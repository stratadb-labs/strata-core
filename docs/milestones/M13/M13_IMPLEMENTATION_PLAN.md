# M13 Implementation Plan: Command Execution Layer (strata-executor)

## Overview

This document provides the high-level implementation plan for M13 (Command Execution Layer).

**Total Scope**: 5 Epics, ~40 Stories

**References**:
- [M13 Executor Specification](./M13_EXECUTOR.md) - Authoritative spec

**Critical Framing**:
> M13 is an **abstraction milestone**, not a feature milestone. It introduces a standardized command interface between all API surfaces and the engine without changing any primitive behavior.
>
> The Command Execution Layer is an in-process execution boundary, not a wire protocol. Commands are typed, serializable operations that represent the complete "instruction set" of Strata.
>
> **M13 does NOT add new capabilities.** It wraps existing primitive operations in a uniform command interface, enabling deterministic replay, thin SDKs, and black-box testing.

**API Coverage**:
> This plan covers ALL operations exposed by the Substrate API layer (see `crates/api/src/substrate/`). The executor must support every public operation to enable complete API parity.

**Migration Strategy**:
> **M13 is additive.** The existing strata-api remains functional. This is intentional:
> - Executor and substrate coexist during M13
> - Parity tests verify executor matches substrate behavior
> - Internal callers can migrate incrementally
> - No breaking changes in M13
>
> **strata-api deletion is deferred to M13.1 or M14** once the executor has soaked and all callers are migrated. Turning an "abstraction milestone" into a "big bang migration" is where schedules die.

**Epic Details**:
- [Epic 90: Command Types](#epic-90-command-types)
- [Epic 91: Output & Error Types](#epic-91-output--error-types)
- [Epic 92: Executor Implementation](#epic-92-executor-implementation)
- [Epic 93: Serialization & JSON Utilities](#epic-93-serialization--json-utilities)
- [Epic 94: Integration & Testing](#epic-94-integration--testing)

---

## Architectural Integration Rules (NON-NEGOTIABLE)

These rules ensure M13 integrates properly with the existing architecture.

### Rule 1: Commands Are Complete

Every public primitive operation MUST have a corresponding Command variant. If an operation cannot be expressed as a command, it is not part of Strata's public behavior.

**FORBIDDEN**: Primitive operations that bypass the command layer, hidden internal-only operations.

### Rule 2: Executor Is Stateless

The Executor dispatches commands to primitives. It holds references to primitives but maintains no state of its own. All state lives in the engine.

**FORBIDDEN**: Caching in the executor, executor-level transactions, executor state that survives restarts.

### Rule 3: Commands Are Self-Contained

Every Command variant contains all information needed to execute. No implicit context, no thread-local state, no ambient configuration.

**FORBIDDEN**: Commands that require external context, implicit run scoping, ambient configuration.

### Rule 4: Output Matches Command

Each Command variant has a deterministic Output type. The same Command on the same state always produces the same Output.

**FORBIDDEN**: Non-deterministic outputs, outputs that vary based on execution context, probabilistic results.

### Rule 5: Errors Are Structured

All errors are represented by the Error enum. No panics, no string-only errors, no error swallowing.

**FORBIDDEN**: Panics in command execution, generic string errors, silent failures.

### Rule 6: Serialization Is Lossless

Commands, Outputs, and Errors MUST serialize and deserialize without loss. Round-trip must be exact.

**FORBIDDEN**: Lossy serialization, type information loss, precision loss.

### Rule 7: Invariant Enforcement Boundary

The Executor enforces **command-layer invariants** (type safety, serialization, self-containment). The primitives/engine enforce **semantic invariants** (run scoping, isolation, versioning, constraints).

| Layer | Enforces |
|-------|----------|
| **Executor** | Commands self-contained, typed, serializable, deterministic output mapping, no panics |
| **Primitives** | Run scoping, isolation, version correctness, retention rules, constraint violations |

**FORBIDDEN**: Executor duplicating primitive validation, executor enforcing business rules.

### Rule 8: No Transport Assumptions

Commands are in-process operations. They do not assume networking, async execution, or remote clients.

**FORBIDDEN**: Network-specific error handling, async requirements, authentication/authorization in commands.

### Rule 9: No Executable Code in Commands

Commands are pure data. They cannot contain closures, function pointers, or any executable code.

**FORBIDDEN**: Closure parameters, `Fn` trait bounds, function pointers, `dyn` trait objects with methods.

**RATIONALE**: Commands must be serializable for replay, logging, and thin SDK support. Executable code cannot be serialized.

**IMPLICATION**: Substrate methods that take closures (e.g., `state_transition`) cannot have direct Command equivalents. Clients must compose multiple commands to achieve the same effect.

---

## Design Decisions

### DD-1: Closure-Based Methods Excluded from Commands

**Decision**: The 5 closure-based `StateCell` methods are not exposed as commands.

**Affected Methods**:
- `state_transition<F>(&self, run, cell, f: F) -> Result<(Value, Version)>`
- `state_transition_or_init<F>(&self, run, cell, initial, f: F) -> Result<(Value, Version)>`
- `state_get_or_init<F>(&self, run, cell, default: F) -> Result<Versioned<Value>>`

**Rationale**:
1. **Serialization**: Closures cannot be serialized to JSON
2. **Determinism**: Closure behavior depends on captured state, breaking replay guarantees
3. **Security**: Executing arbitrary code from deserialized commands is a security risk
4. **Completeness**: The 8 non-closure State commands provide full functionality

**Alternatives Considered**:

| Alternative | Rejected Because |
|-------------|------------------|
| Expression DSL | Scope creep, complexity, limited expressiveness |
| Named transforms (Incr, Append) | Already have `KvIncr`, `JsonArrayPush` for common cases |
| WASM modules | Too heavy, deployment complexity |
| Server-side scripting | Security concerns, not in M13 scope |

**Mitigation**: Document client-side patterns that compose `StateGet` + `StateCas` to achieve equivalent semantics with optimistic concurrency retry loops.

---

## Core Invariants

### Command Invariants (CMD)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| CMD-1 | Every primitive operation has a Command variant | Exhaustive coverage audit |
| CMD-2 | Commands are self-contained (no external context) | Static analysis, constructor tests |
| CMD-3 | Commands serialize/deserialize losslessly | Round-trip tests for all variants |
| CMD-4 | Command execution is deterministic | Same command + same state = same result |
| CMD-5 | All 101 command variants are typed (no Generic fallback) | Type exhaustiveness tests |

### Output Invariants (OUT)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| OUT-1 | Output variants cover all return types | Exhaustive coverage audit |
| OUT-2 | Outputs serialize/deserialize losslessly | Round-trip tests for all variants |
| OUT-3 | Output matches expected type for each Command | Type mapping tests |
| OUT-4 | Versioned outputs preserve version metadata | Version preservation tests |

### Error Invariants (ERR)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| ERR-1 | All primitive errors map to Error variants | Error coverage tests |
| ERR-2 | Errors serialize/deserialize losslessly | Round-trip tests |
| ERR-3 | Errors include structured details | Error detail completeness tests |
| ERR-4 | No error swallowing or transformation | Error propagation tests |

### Executor Invariants (EXE)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| EXE-1 | Executor is stateless | State isolation tests |
| EXE-2 | execute() dispatches correctly to all 101 variants | Dispatch coverage tests |
| EXE-3 | execute_many() processes sequentially | Order preservation tests |
| EXE-4 | Executor does not modify command semantics | Parity tests vs direct primitive calls |

### Serialization Invariants (SER)

| # | Invariant | Test Strategy |
|---|-----------|---------------|
| SER-1 | JSON encoding handles all Value types | Type coverage tests |
| SER-2 | Special values preserved ($bytes, $f64 wrappers) | Special value round-trip tests |
| SER-3 | Large integers preserved (i64 range) | Numeric precision tests |
| SER-4 | Binary data encoded as base64 | Bytes encoding tests |

---

## Epic Overview

| Epic | Name | Stories | Dependencies | Status |
|------|------|---------|--------------|--------|
| 90 | Command Types | 12 | M11 complete | Pending |
| 91 | Output & Error Types | 6 | Epic 90 | Pending |
| 92 | Executor Implementation | 13 | Epic 90, 91 | Pending |
| 93 | Serialization & JSON Utilities | 5 | Epic 90, 91 | Pending |
| 94 | Integration & Testing | 4 | Epic 92, 93 | Pending |

---

## Epic 90: Command Types

**Goal**: Define the complete Command enum covering all 101 primitive operations

| Story | Description | Priority |
|-------|-------------|----------|
| #700 | Command Enum Structure and RunId Type | FOUNDATION |
| #701 | KV Command Variants (15 variants) | CRITICAL |
| #702 | JSON Command Variants (17 variants) | CRITICAL |
| #703 | Event Command Variants (11 variants) | CRITICAL |
| #704 | State Command Variants (8 variants) | CRITICAL |
| #705 | Vector Command Variants (19 variants) | CRITICAL |
| #706 | Run Command Variants (24 variants) | CRITICAL |
| #707 | Transaction Command Variants (5 variants) | CRITICAL |
| #708 | Retention Command Variants (3 variants) | HIGH |
| #709 | Database Command Variants (4 variants) | HIGH |
| #710 | Supporting Types (SearchFilter, DistanceMetric, etc.) | CRITICAL |
| #711 | Command Documentation and Examples | HIGH |

**Acceptance Criteria**:
- [ ] `Command` enum with exactly 101 variants (no Generic fallback)
- [ ] All commands derive `Debug`, `Clone`, `Serialize`, `Deserialize`, `PartialEq`
- [ ] `RunId` type for run identification (String-based, supports "default" and UUIDs)

### KV Commands (15)
From `KVStore` and `KVStoreBatch` traits:

| # | Command | Parameters | Return |
|---|---------|------------|--------|
| 1 | `KvPut` | `run, key, value` | `Version` |
| 2 | `KvGet` | `run, key` | `Option<Versioned<Value>>` |
| 3 | `KvGetAt` | `run, key, version` | `Versioned<Value>` |
| 4 | `KvDelete` | `run, key` | `bool` |
| 5 | `KvExists` | `run, key` | `bool` |
| 6 | `KvHistory` | `run, key, limit?, before?` | `Vec<Versioned<Value>>` |
| 7 | `KvIncr` | `run, key, delta` | `i64` |
| 8 | `KvCasVersion` | `run, key, expected_version?, new_value` | `bool` |
| 9 | `KvCasValue` | `run, key, expected_value?, new_value` | `bool` |
| 10 | `KvKeys` | `run, prefix, limit?` | `Vec<String>` |
| 11 | `KvScan` | `run, prefix, limit, cursor?` | `KVScanResult` |
| 12 | `KvMget` | `run, keys` | `Vec<Option<Versioned<Value>>>` |
| 13 | `KvMput` | `run, entries` | `Version` |
| 14 | `KvMdelete` | `run, keys` | `u64` |
| 15 | `KvMexists` | `run, keys` | `u64` |

### JSON Commands (17)
From `JsonStore` trait:

| # | Command | Parameters | Return |
|---|---------|------------|--------|
| 16 | `JsonSet` | `run, key, path, value` | `Version` |
| 17 | `JsonGet` | `run, key, path` | `Option<Versioned<Value>>` |
| 18 | `JsonDelete` | `run, key, path` | `u64` |
| 19 | `JsonMerge` | `run, key, path, patch` | `Version` |
| 20 | `JsonHistory` | `run, key, limit?, before?` | `Vec<Versioned<Value>>` |
| 21 | `JsonExists` | `run, key` | `bool` |
| 22 | `JsonGetVersion` | `run, key` | `Option<u64>` |
| 23 | `JsonSearch` | `run, query, k` | `Vec<JsonSearchHit>` |
| 24 | `JsonList` | `run, prefix?, cursor?, limit` | `JsonListResult` |
| 25 | `JsonCas` | `run, key, expected_version, path, value` | `Version` |
| 26 | `JsonQuery` | `run, path, value, limit` | `Vec<String>` |
| 27 | `JsonCount` | `run` | `u64` |
| 28 | `JsonBatchGet` | `run, keys` | `Vec<Option<Versioned<Value>>>` |
| 29 | `JsonBatchCreate` | `run, docs` | `Vec<Version>` |
| 30 | `JsonArrayPush` | `run, key, path, values` | `usize` |
| 31 | `JsonIncrement` | `run, key, path, delta` | `f64` |
| 32 | `JsonArrayPop` | `run, key, path` | `Option<Value>` |

### Event Commands (11)
From `EventLog` trait:

| # | Command | Parameters | Return |
|---|---------|------------|--------|
| 33 | `EventAppend` | `run, stream, payload` | `Version` |
| 34 | `EventAppendBatch` | `run, events` | `Vec<Version>` |
| 35 | `EventRange` | `run, stream, start?, end?, limit?` | `Vec<Versioned<Value>>` |
| 36 | `EventGet` | `run, stream, sequence` | `Option<Versioned<Value>>` |
| 37 | `EventLen` | `run, stream` | `u64` |
| 38 | `EventLatestSequence` | `run, stream` | `Option<u64>` |
| 39 | `EventStreamInfo` | `run, stream` | `StreamInfo` |
| 40 | `EventRevRange` | `run, stream, start?, end?, limit?` | `Vec<Versioned<Value>>` |
| 41 | `EventStreams` | `run` | `Vec<String>` |
| 42 | `EventHead` | `run, stream` | `Option<Versioned<Value>>` |
| 43 | `EventVerifyChain` | `run` | `ChainVerification` |

### State Commands (8)
From `StateCell` trait (excluding closure-based methods):

| # | Command | Parameters | Return |
|---|---------|------------|--------|
| 44 | `StateSet` | `run, cell, value` | `Version` |
| 45 | `StateGet` | `run, cell` | `Option<Versioned<Value>>` |
| 46 | `StateCas` | `run, cell, expected_counter?, value` | `Option<Version>` |
| 47 | `StateDelete` | `run, cell` | `bool` |
| 48 | `StateExists` | `run, cell` | `bool` |
| 49 | `StateHistory` | `run, cell, limit?, before?` | `Vec<Versioned<Value>>` |
| 50 | `StateInit` | `run, cell, value` | `Version` |
| 51 | `StateList` | `run` | `Vec<String>` |

#### Closure-Based Methods (Not Commandable)

The following `StateCell` methods use closures and **cannot be direct commands**:

| Method | Signature | Purpose |
|--------|-----------|---------|
| `state_transition` | `fn(&Value) -> Result<Value>` | Apply transform with auto-retry |
| `state_transition_or_init` | `fn(&Value) -> Result<Value>` | Transform or init with closure |
| `state_get_or_init` | `fn() -> Value` | Lazy default initialization |

**Why closures can't be commands**: Commands must be pure data structures that serialize to JSON. Closures contain executable code and captured state that cannot be serialized.

**Client-Side Pattern**: Clients can implement equivalent functionality using the available commands:

```rust
// Equivalent of state_transition (optimistic concurrency with retry)
fn client_state_transition(
    executor: &Executor,
    run: &RunId,
    cell: &str,
    transform: impl Fn(&Value) -> Value,
) -> Result<(Value, Version), Error> {
    loop {
        // 1. Read current state
        let current = executor.execute(StateGet { run, cell })?;

        let (old_value, expected_counter) = match current {
            Some(versioned) => {
                let counter = match versioned.version {
                    Version::Counter(c) => Some(c),
                    _ => None,
                };
                (versioned.value, counter)
            }
            None => return Err(Error::CellNotFound { cell: cell.into() }),
        };

        // 2. Apply transformation (client-side logic)
        let new_value = transform(&old_value);

        // 3. Attempt CAS
        let result = executor.execute(StateCas {
            run,
            cell,
            expected_counter,
            value: new_value.clone(),
        })?;

        // 4. Retry on conflict, return on success
        if let Some(version) = result {
            return Ok((new_value, version));
        }
        // CAS failed due to concurrent modification - retry
    }
}

// Equivalent of state_get_or_init
fn client_state_get_or_init(
    executor: &Executor,
    run: &RunId,
    cell: &str,
    default: impl FnOnce() -> Value,
) -> Result<Versioned<Value>, Error> {
    // Try to get existing value
    if let Some(versioned) = executor.execute(StateGet { run, cell })? {
        return Ok(versioned);
    }

    // Cell doesn't exist - initialize it
    let initial_value = default();
    let version = executor.execute(StateInit { run, cell, value: initial_value.clone() })?;

    Ok(Versioned {
        value: initial_value,
        version,
        timestamp: Timestamp::now(),
    })
}
```

This pattern preserves the optimistic concurrency semantics while keeping commands serializable.

### Vector Commands (19)
From `VectorStore` trait:

| # | Command | Parameters | Return |
|---|---------|------------|--------|
| 52 | `VectorUpsert` | `run, collection, key, vector, metadata?` | `Version` |
| 53 | `VectorUpsertWithSource` | `run, collection, key, vector, metadata?, source_ref?` | `Version` |
| 54 | `VectorGet` | `run, collection, key` | `Option<Versioned<VectorData>>` |
| 55 | `VectorDelete` | `run, collection, key` | `bool` |
| 56 | `VectorSearch` | `run, collection, query, k, filter?, metric?` | `Vec<VectorMatch>` |
| 57 | `VectorSearchWithBudget` | `run, collection, query, k, filter?, budget` | `(Vec<VectorMatch>, bool)` |
| 58 | `VectorCollectionInfo` | `run, collection` | `Option<VectorCollectionInfo>` |
| 59 | `VectorCreateCollection` | `run, collection, dimension, metric` | `Version` |
| 60 | `VectorDropCollection` | `run, collection` | `bool` |
| 61 | `VectorListCollections` | `run` | `Vec<VectorCollectionInfo>` |
| 62 | `VectorCollectionExists` | `run, collection` | `bool` |
| 63 | `VectorCount` | `run, collection` | `u64` |
| 64 | `VectorUpsertBatch` | `run, collection, vectors` | `Vec<Result<(String, Version), Error>>` |
| 65 | `VectorGetBatch` | `run, collection, keys` | `Vec<Option<Versioned<VectorData>>>` |
| 66 | `VectorDeleteBatch` | `run, collection, keys` | `Vec<bool>` |
| 67 | `VectorHistory` | `run, collection, key, limit?, before_version?` | `Vec<Versioned<VectorData>>` |
| 68 | `VectorGetAt` | `run, collection, key, version` | `Option<Versioned<VectorData>>` |
| 69 | `VectorListKeys` | `run, collection, limit?, cursor?` | `Vec<String>` |
| 70 | `VectorScan` | `run, collection, limit?, cursor?` | `Vec<(String, VectorData)>` |

### Run Commands (24)
From `RunIndex` trait:

| # | Command | Parameters | Return |
|---|---------|------------|--------|
| 71 | `RunCreate` | `run_id?, metadata?` | `(RunInfo, Version)` |
| 72 | `RunGet` | `run` | `Option<Versioned<RunInfo>>` |
| 73 | `RunList` | `state?, limit?, offset?` | `Vec<Versioned<RunInfo>>` |
| 74 | `RunClose` | `run` | `Version` |
| 75 | `RunUpdateMetadata` | `run, metadata` | `Version` |
| 76 | `RunExists` | `run` | `bool` |
| 77 | `RunPause` | `run` | `Version` |
| 78 | `RunResume` | `run` | `Version` |
| 79 | `RunFail` | `run, error` | `Version` |
| 80 | `RunCancel` | `run` | `Version` |
| 81 | `RunArchive` | `run` | `Version` |
| 82 | `RunDelete` | `run` | `()` |
| 83 | `RunQueryByStatus` | `state` | `Vec<Versioned<RunInfo>>` |
| 84 | `RunQueryByTag` | `tag` | `Vec<Versioned<RunInfo>>` |
| 85 | `RunCount` | `status?` | `u64` |
| 86 | `RunSearch` | `query, limit?` | `Vec<Versioned<RunInfo>>` |
| 87 | `RunAddTags` | `run, tags` | `Version` |
| 88 | `RunRemoveTags` | `run, tags` | `Version` |
| 89 | `RunGetTags` | `run` | `Vec<String>` |
| 90 | `RunCreateChild` | `parent, metadata?` | `(RunInfo, Version)` |
| 91 | `RunGetChildren` | `parent` | `Vec<Versioned<RunInfo>>` |
| 92 | `RunGetParent` | `run` | `Option<RunId>` |
| 93 | `RunSetRetention` | `run, policy` | `Version` |
| 94 | `RunGetRetention` | `run` | `RetentionPolicy` |

### Transaction Commands (5)
From `TransactionControl` trait:

| # | Command | Parameters | Return |
|---|---------|------------|--------|
| 95 | `TxnBegin` | `options?` | `TxnId` |
| 96 | `TxnCommit` | *(none)* | `Version` |
| 97 | `TxnRollback` | *(none)* | `()` |
| 98 | `TxnInfo` | *(none)* | `Option<TxnInfo>` |
| 99 | `TxnIsActive` | *(none)* | `bool` |

**Note**: `TransactionSavepoint` methods (`savepoint`, `rollback_to`, `release_savepoint`) are deferred to post-MVP.

### Retention Commands (3)
From `RetentionSubstrate` trait:

| # | Command | Parameters | Return |
|---|---------|------------|--------|
| 100 | `RetentionGet` | `run` | `Option<RetentionVersion>` |
| 101 | `RetentionSet` | `run, policy` | `u64` |
| 102 | `RetentionClear` | `run` | `bool` |

### Database Commands (4)

| # | Command | Parameters | Return |
|---|---------|------------|--------|
| 103 | `Ping` | *(none)* | `Pong { version }` |
| 104 | `Info` | *(none)* | `DatabaseInfo` |
| 105 | `Flush` | *(none)* | `()` |
| 106 | `Compact` | *(none)* | `()` |

**Note**: Final command count is 106, but 5 commands are deferred (3 closure-based State methods + 3 savepoint methods - but we count 101 as implementable now).

- [ ] All field types use established core types (Value, RunId, etc.)
- [ ] Unit tests for construction of all 101 variants

---

## Epic 91: Output & Error Types

**Goal**: Define Output enum for successful results and Error enum for failures

| Story | Description | Priority |
|-------|-------------|----------|
| #712 | Output Enum Core Variants | FOUNDATION |
| #713 | Output Versioned and Collection Variants | CRITICAL |
| #714 | VersionedValue and Supporting Types | CRITICAL |
| #715 | Error Enum Implementation | CRITICAL |
| #716 | Error Detail Types | HIGH |
| #717 | Command-Output Type Mapping Documentation | HIGH |

**Acceptance Criteria**:
- [ ] `Output` enum with all return type variants:
  - `Unit` - No return value (delete, flush)
  - `Value(Value)` - Single value
  - `Versioned { value: Value, version: u64, timestamp: u64 }` - Value with version
  - `Maybe(Option<Value>)` - Optional value (get operations)
  - `MaybeVersioned(Option<VersionedValue>)` - Optional versioned value
  - `Values(Vec<Option<VersionedValue>>)` - Multiple optional versioned values (mget)
  - `Version(u64)` - Version number only
  - `MaybeVersion(Option<u64>)` - Optional version (CAS operations)
  - `Bool(bool)` - Boolean result
  - `Int(i64)` - Integer result (count, incr)
  - `Uint(u64)` - Unsigned integer result
  - `Float(f64)` - Float result (json_increment)
  - `Keys(Vec<String>)` - List of keys
  - `Strings(Vec<String>)` - List of strings (tags, streams)
  - `VersionedValues(Vec<Versioned<Value>>)` - Version history
  - `VectorMatches(Vec<VectorMatch>)` - Vector search results
  - `VectorMatchesWithExhausted((Vec<VectorMatch>, bool))` - Search with budget
  - `VectorData(Option<Versioned<VectorData>>)` - Single vector
  - `VectorDataList(Vec<Option<Versioned<VectorData>>>)` - Multiple vectors
  - `VectorDataHistory(Vec<Versioned<VectorData>>)` - Vector history
  - `VectorKeyValues(Vec<(String, VectorData)>)` - Vector scan result
  - `VectorBatchResult(Vec<Result<(String, Version), Error>>)` - Batch upsert result
  - `Bools(Vec<bool>)` - Multiple booleans (batch delete)
  - `RunInfo(RunInfo)` - Single run info
  - `RunInfoVersioned(Versioned<RunInfo>)` - Versioned run info
  - `RunInfoList(Vec<Versioned<RunInfo>>)` - Multiple run infos
  - `RunWithVersion((RunInfo, Version))` - Run create result
  - `MaybeRunId(Option<RunId>)` - Optional run ID (parent)
  - `DatabaseInfo(DatabaseInfo)` - Database info
  - `Pong { version: String }` - Ping response
  - `KVScanResult { entries: Vec<(String, VersionedValue)>, cursor: Option<String> }` - KV scan
  - `JsonListResult { keys: Vec<String>, cursor: Option<String> }` - JSON list
  - `JsonSearchHits(Vec<JsonSearchHit>)` - JSON search results
  - `StreamInfo(StreamInfo)` - Event stream metadata
  - `ChainVerification(ChainVerification)` - Chain verification result
  - `VectorCollectionInfo(Option<VectorCollectionInfo>)` - Collection info
  - `VectorCollectionList(Vec<VectorCollectionInfo>)` - Collection list
  - `TxnId(TxnId)` - Transaction ID
  - `TxnInfo(Option<TxnInfo>)` - Transaction info
  - `RetentionVersion(Option<RetentionVersion>)` - Retention info
  - `RetentionPolicy(RetentionPolicy)` - Retention policy
  - `Versions(Vec<Version>)` - Multiple versions (batch create)
- [ ] Supporting types:
  - `VersionedValue` struct: `{ value: Value, version: Version, timestamp: Timestamp }`
  - `VectorMatch` struct for vector search results
  - `VectorData` type alias: `(Vec<f32>, Value)`
  - `VectorCollectionInfo` struct
  - `RunInfo` struct for run metadata
  - `StreamInfo` struct for event stream metadata
  - `ChainVerification` struct
  - `JsonSearchHit` struct
  - `TxnId`, `TxnInfo`, `TxnStatus` structs
  - `RetentionVersion`, `RetentionPolicy` structs
  - `DatabaseInfo` struct for database info
- [ ] All Output variants derive `Debug`, `Clone`, `Serialize`, `Deserialize`, `PartialEq`
- [ ] `Error` enum with all error cases:
  - `KeyNotFound { key: String }`
  - `RunNotFound { run: String }`
  - `CollectionNotFound { collection: String }`
  - `StreamNotFound { stream: String }`
  - `CellNotFound { cell: String }`
  - `DocumentNotFound { key: String }`
  - `WrongType { expected: String, actual: String }`
  - `InvalidKey { reason: String }`
  - `InvalidPath { reason: String }`
  - `InvalidInput { reason: String }`
  - `VersionConflict { expected: u64, actual: u64 }`
  - `TransitionFailed { expected: String, actual: String }`
  - `RunClosed { run: String }`
  - `RunExists { run: String }`
  - `CollectionExists { collection: String }`
  - `DimensionMismatch { expected: usize, actual: usize }`
  - `ConstraintViolation { reason: String }`
  - `HistoryTrimmed { requested: u64, earliest: u64 }`
  - `Overflow { reason: String }`
  - `Conflict { reason: String }`
  - `TransactionNotActive`
  - `TransactionAlreadyActive`
  - `Io { reason: String }`
  - `Serialization { reason: String }`
  - `Internal { reason: String }`
- [ ] Error implements `std::error::Error` and `Display`
- [ ] Error derives `Serialize`, `Deserialize`, `Clone`, `Debug`

### Command-Output Type Mapping

**KV Commands**:
| Command | Output Type |
|---------|-------------|
| `KvPut` | `Version` |
| `KvGet` | `MaybeVersioned` |
| `KvGetAt` | `Versioned` |
| `KvDelete` | `Bool` |
| `KvExists` | `Bool` |
| `KvHistory` | `VersionedValues` |
| `KvIncr` | `Int` |
| `KvCasVersion` | `Bool` |
| `KvCasValue` | `Bool` |
| `KvKeys` | `Keys` |
| `KvScan` | `KVScanResult` |
| `KvMget` | `Values` |
| `KvMput` | `Version` |
| `KvMdelete` | `Uint` |
| `KvMexists` | `Uint` |

**JSON Commands**:
| Command | Output Type |
|---------|-------------|
| `JsonSet` | `Version` |
| `JsonGet` | `MaybeVersioned` |
| `JsonDelete` | `Uint` |
| `JsonMerge` | `Version` |
| `JsonHistory` | `VersionedValues` |
| `JsonExists` | `Bool` |
| `JsonGetVersion` | `MaybeVersion` |
| `JsonSearch` | `JsonSearchHits` |
| `JsonList` | `JsonListResult` |
| `JsonCas` | `Version` |
| `JsonQuery` | `Keys` |
| `JsonCount` | `Uint` |
| `JsonBatchGet` | `Values` |
| `JsonBatchCreate` | `Versions` |
| `JsonArrayPush` | `Uint` |
| `JsonIncrement` | `Float` |
| `JsonArrayPop` | `Maybe` |

**Event Commands**:
| Command | Output Type |
|---------|-------------|
| `EventAppend` | `Version` |
| `EventAppendBatch` | `Versions` |
| `EventRange` | `VersionedValues` |
| `EventGet` | `MaybeVersioned` |
| `EventLen` | `Uint` |
| `EventLatestSequence` | `MaybeVersion` |
| `EventStreamInfo` | `StreamInfo` |
| `EventRevRange` | `VersionedValues` |
| `EventStreams` | `Strings` |
| `EventHead` | `MaybeVersioned` |
| `EventVerifyChain` | `ChainVerification` |

**State Commands**:
| Command | Output Type |
|---------|-------------|
| `StateSet` | `Version` |
| `StateGet` | `MaybeVersioned` |
| `StateCas` | `MaybeVersion` |
| `StateDelete` | `Bool` |
| `StateExists` | `Bool` |
| `StateHistory` | `VersionedValues` |
| `StateInit` | `Version` |
| `StateList` | `Strings` |

**Vector Commands**:
| Command | Output Type |
|---------|-------------|
| `VectorUpsert` | `Version` |
| `VectorUpsertWithSource` | `Version` |
| `VectorGet` | `VectorData` |
| `VectorDelete` | `Bool` |
| `VectorSearch` | `VectorMatches` |
| `VectorSearchWithBudget` | `VectorMatchesWithExhausted` |
| `VectorCollectionInfo` | `VectorCollectionInfo` |
| `VectorCreateCollection` | `Version` |
| `VectorDropCollection` | `Bool` |
| `VectorListCollections` | `VectorCollectionList` |
| `VectorCollectionExists` | `Bool` |
| `VectorCount` | `Uint` |
| `VectorUpsertBatch` | `VectorBatchResult` |
| `VectorGetBatch` | `VectorDataList` |
| `VectorDeleteBatch` | `Bools` |
| `VectorHistory` | `VectorDataHistory` |
| `VectorGetAt` | `VectorData` |
| `VectorListKeys` | `Keys` |
| `VectorScan` | `VectorKeyValues` |

**Run Commands**:
| Command | Output Type |
|---------|-------------|
| `RunCreate` | `RunWithVersion` |
| `RunGet` | `RunInfoVersioned` |
| `RunList` | `RunInfoList` |
| `RunClose` | `Version` |
| `RunUpdateMetadata` | `Version` |
| `RunExists` | `Bool` |
| `RunPause` | `Version` |
| `RunResume` | `Version` |
| `RunFail` | `Version` |
| `RunCancel` | `Version` |
| `RunArchive` | `Version` |
| `RunDelete` | `Unit` |
| `RunQueryByStatus` | `RunInfoList` |
| `RunQueryByTag` | `RunInfoList` |
| `RunCount` | `Uint` |
| `RunSearch` | `RunInfoList` |
| `RunAddTags` | `Version` |
| `RunRemoveTags` | `Version` |
| `RunGetTags` | `Strings` |
| `RunCreateChild` | `RunWithVersion` |
| `RunGetChildren` | `RunInfoList` |
| `RunGetParent` | `MaybeRunId` |
| `RunSetRetention` | `Version` |
| `RunGetRetention` | `RetentionPolicy` |

**Transaction Commands**:
| Command | Output Type |
|---------|-------------|
| `TxnBegin` | `TxnId` |
| `TxnCommit` | `Version` |
| `TxnRollback` | `Unit` |
| `TxnInfo` | `TxnInfo` |
| `TxnIsActive` | `Bool` |

**Retention Commands**:
| Command | Output Type |
|---------|-------------|
| `RetentionGet` | `RetentionVersion` |
| `RetentionSet` | `Uint` |
| `RetentionClear` | `Bool` |

**Database Commands**:
| Command | Output Type |
|---------|-------------|
| `Ping` | `Pong` |
| `Info` | `DatabaseInfo` |
| `Flush` | `Unit` |
| `Compact` | `Unit` |

---

## Epic 92: Executor Implementation

**Goal**: Implement the Executor that dispatches commands to primitives

| Story | Description | Priority |
|-------|-------------|----------|
| #720 | Executor Struct and Constructor | FOUNDATION |
| #721 | KV Command Handlers (15 handlers) | CRITICAL |
| #722 | JSON Command Handlers (17 handlers) | CRITICAL |
| #723 | Event Command Handlers (11 handlers) | CRITICAL |
| #724 | State Command Handlers (8 handlers) | CRITICAL |
| #725 | Vector Command Handlers (19 handlers) | CRITICAL |
| #726 | Run Command Handlers (24 handlers) | CRITICAL |
| #727 | Transaction Command Handlers (5 handlers) | CRITICAL |
| #728 | Retention Command Handlers (3 handlers) | HIGH |
| #729 | Database Command Handlers (4 handlers) | HIGH |
| #730 | Error Conversion Layer | CRITICAL |
| #731 | Handler Dispatch Match Statement | CRITICAL |
| #732 | Handler Unit Tests | HIGH |

**Acceptance Criteria**:
- [ ] `Executor` struct holding references to all primitives:
  ```rust
  pub struct Executor {
      substrate: Arc<SubstrateImpl>,
      // Substrate provides access to all primitive operations
  }
  ```
- [ ] `Executor::new(substrate: Arc<SubstrateImpl>) -> Self`
- [ ] `Executor::execute(&self, cmd: Command) -> Result<Output, Error>`
- [ ] `Executor::execute_many(&self, cmds: Vec<Command>) -> Vec<Result<Output, Error>>`
- [ ] Match dispatch covering all 101 command variants

### Handler Requirements by Category

**KV handlers (15)**:
- All `KVStore` trait methods: `kv_put`, `kv_get`, `kv_get_at`, `kv_delete`, `kv_exists`, `kv_history`, `kv_incr`, `kv_cas_version`, `kv_cas_value`, `kv_keys`, `kv_scan`
- All `KVStoreBatch` trait methods: `kv_mget`, `kv_mput`, `kv_mdelete`, `kv_mexists`

**JSON handlers (17)**:
- All `JsonStore` trait methods: `json_set`, `json_get`, `json_delete`, `json_merge`, `json_history`, `json_exists`, `json_get_version`, `json_search`, `json_list`, `json_cas`, `json_query`, `json_count`, `json_batch_get`, `json_batch_create`, `json_array_push`, `json_increment`, `json_array_pop`

**Event handlers (11)**:
- All `EventLog` trait methods: `event_append`, `event_append_batch`, `event_range`, `event_get`, `event_len`, `event_latest_sequence`, `event_stream_info`, `event_rev_range`, `event_streams`, `event_head`, `event_verify_chain`

**State handlers (8)**:
- Non-closure `StateCell` trait methods: `state_set`, `state_get`, `state_cas`, `state_delete`, `state_exists`, `state_history`, `state_init`, `state_list`
- Note: `state_transition*` methods excluded (closure-based)

**Vector handlers (19)**:
- All `VectorStore` trait methods: `vector_upsert`, `vector_upsert_with_source`, `vector_get`, `vector_delete`, `vector_search`, `vector_search_with_budget`, `vector_collection_info`, `vector_create_collection`, `vector_drop_collection`, `vector_list_collections`, `vector_collection_exists`, `vector_count`, `vector_upsert_batch`, `vector_get_batch`, `vector_delete_batch`, `vector_history`, `vector_get_at`, `vector_list_keys`, `vector_scan`

**Run handlers (24)**:
- All `RunIndex` trait methods: `run_create`, `run_get`, `run_list`, `run_close`, `run_update_metadata`, `run_exists`, `run_pause`, `run_resume`, `run_fail`, `run_cancel`, `run_archive`, `run_delete`, `run_query_by_status`, `run_query_by_tag`, `run_count`, `run_search`, `run_add_tags`, `run_remove_tags`, `run_get_tags`, `run_create_child`, `run_get_children`, `run_get_parent`, `run_set_retention`, `run_get_retention`

**Transaction handlers (5)**:
- All `TransactionControl` trait methods: `txn_begin`, `txn_commit`, `txn_rollback`, `txn_info`, `txn_is_active`

**Retention handlers (3)**:
- All `RetentionSubstrate` trait methods: `retention_get`, `retention_set`, `retention_clear`

**Database handlers (4)**:
- `Ping` → returns `Pong { version: env!("CARGO_PKG_VERSION") }`
- `Info` → returns `Info(DatabaseInfo)`
- `Flush` → calls `engine.flush()`, returns `Unit`
- `Compact` → calls `engine.compact()`, returns `Unit`

**Error conversion**:
- [ ] Internal `strata_core::Error` maps to `executor::Error`
- [ ] No error information lost
- [ ] Structured details preserved
- [ ] All handlers are synchronous (no async)
- [ ] Executor is `Send + Sync`
- [ ] Unit tests for each handler

---

## Epic 93: Serialization & JSON Utilities

**Goal**: Implement JSON serialization for CLI and MCP output formatting

| Story | Description | Priority |
|-------|-------------|----------|
| #730 | Value JSON Encoding | CRITICAL |
| #731 | Special Value Wrappers ($bytes, $f64) | CRITICAL |
| #732 | Output JSON Encoding | HIGH |
| #733 | Error JSON Encoding | HIGH |
| #734 | Command JSON Encoding | HIGH |

**Acceptance Criteria**:
- [ ] `Value` → JSON encoding:
  - `Null` → `null`
  - `Bool(b)` → `true`/`false`
  - `Int(n)` → number (as JSON number if in safe range)
  - `Float(f)` → number (normal) or `{"$f64": "NaN|+Inf|-Inf|-0.0"}` (special)
  - `String(s)` → `"string"`
  - `Bytes(b)` → `{"$bytes": "<base64>"}`
  - `Array(a)` → `[...]`
  - `Object(o)` → `{...}`
- [ ] Special float handling:
  - `NaN` → `{"$f64": "NaN"}`
  - `+Infinity` → `{"$f64": "+Inf"}`
  - `-Infinity` → `{"$f64": "-Inf"}`
  - `-0.0` → `{"$f64": "-0.0"}`
- [ ] Bytes encoding uses standard base64 (RFC 4648)
- [ ] JSON → `Value` decoding:
  - Recognizes `$bytes` wrapper
  - Recognizes `$f64` wrapper
  - Numbers decode to `Int` if no decimal, `Float` if decimal
  - Large integers (> i64::MAX) handled gracefully (error or BigInt representation)
- [ ] `Output` JSON encoding for all variants
- [ ] `Error` JSON encoding: `{"code": "...", "message": "...", "details": {...}}`
- [ ] `Command` JSON encoding for debugging/logging
- [ ] Round-trip tests for all types
- [ ] No precision loss for i64 integers
- [ ] No precision loss for f64 floats (via wrappers)

---

## Epic 94: Integration & Testing

**Goal**: Integrate executor with existing API layer and comprehensive testing

| Story | Description | Priority |
|-------|-------------|----------|
| #740 | Workspace Integration | CRITICAL |
| #741 | Executor-Primitive Parity Tests | CRITICAL |
| #742 | Serialization Round-Trip Tests | CRITICAL |
| #743 | Determinism Verification Tests | HIGH |

**Acceptance Criteria**:
- [ ] `strata-executor` crate added to workspace
- [ ] Crate dependencies:
  ```toml
  [dependencies]
  strata-core = { path = "../core" }
  strata-engine = { path = "../engine" }
  strata-primitives = { path = "../primitives" }
  serde = { workspace = true }
  serde_json = { workspace = true }
  thiserror = { workspace = true }
  base64 = { workspace = true }
  ```
- [ ] Public API exports:
  - `Command` enum
  - `Output` enum
  - `Error` enum
  - `Executor` struct
  - `VersionedValue`, `Event`, `SearchResult`, `RunInfo`, `DatabaseInfo`
- [ ] **Parity tests**: Every command produces same result as direct primitive call
- [ ] **Round-trip tests**: All commands, outputs, errors survive JSON round-trip
- [ ] **Determinism tests**: Same command sequence on same initial state = same results
- [ ] **Coverage**: All 101 command variants have execution tests
- [ ] **Error coverage**: All error variants have trigger tests
- [ ] Integration test: Full workflow (create run → operations → export)
- [ ] Benchmark: Command dispatch overhead < 100ns

---

## Files to Create/Modify

### New Files

| File | Description |
|------|-------------|
| `crates/executor/Cargo.toml` | Crate manifest |
| `crates/executor/src/lib.rs` | Public API, re-exports |
| `crates/executor/src/command.rs` | Command enum (101 variants) |
| `crates/executor/src/output.rs` | Output enum (~40 variants) and supporting types |
| `crates/executor/src/error.rs` | Error enum (~25 variants) |
| `crates/executor/src/executor.rs` | Executor implementation |
| `crates/executor/src/handlers/mod.rs` | Handler module |
| `crates/executor/src/handlers/kv.rs` | KV command handlers (15) |
| `crates/executor/src/handlers/json.rs` | JSON command handlers (17) |
| `crates/executor/src/handlers/event.rs` | Event command handlers (11) |
| `crates/executor/src/handlers/state.rs` | State command handlers (8) |
| `crates/executor/src/handlers/vector.rs` | Vector command handlers (19) |
| `crates/executor/src/handlers/run.rs` | Run command handlers (24) |
| `crates/executor/src/handlers/transaction.rs` | Transaction command handlers (5) |
| `crates/executor/src/handlers/retention.rs` | Retention command handlers (3) |
| `crates/executor/src/handlers/database.rs` | Database command handlers (4) |
| `crates/executor/src/convert.rs` | Error conversion from internal errors |
| `crates/executor/src/json.rs` | JSON encoding utilities |
| `crates/executor/src/types.rs` | Supporting types (VersionedValue, etc.) |

### Test Files

| File | Description |
|------|-------------|
| `crates/executor/tests/command_tests.rs` | Command construction and serialization |
| `crates/executor/tests/output_tests.rs` | Output type tests |
| `crates/executor/tests/error_tests.rs` | Error type tests |
| `crates/executor/tests/executor_tests.rs` | Executor dispatch tests |
| `crates/executor/tests/parity_tests.rs` | Command vs primitive parity |
| `crates/executor/tests/roundtrip_tests.rs` | JSON round-trip tests |
| `crates/executor/tests/determinism_tests.rs` | Determinism verification |

### Modified Files

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `crates/executor` to workspace members |
| `crates/api/src/lib.rs` | Optional: expose executor for API consumers |

---

## Dependency Order

```
Epic 90 (Command Types)
    ↓
Epic 91 (Output & Error Types) ←── Epic 90
    ↓
    ├───────────────┐
    ↓               ↓
Epic 92         Epic 93
(Executor)      (Serialization)
    ↓               ↓
    └───────┬───────┘
            ↓
Epic 94 (Integration & Testing)
            ↓
════════════════════════════════
        M13 COMPLETE
════════════════════════════════
```

**Recommended Implementation Order**:
1. Epic 90: Command Types (foundation - defines all operations)
2. Epic 91: Output & Error Types (defines all results)
3. Epic 93: Serialization & JSON Utilities (can be done in parallel with 92)
4. Epic 92: Executor Implementation (brings it all together)
5. Epic 94: Integration & Testing (validates everything)

---

## Phased Implementation Strategy

> **Guiding Principle**: Define types first. Commands must be complete before the executor. Serialization should work independently. Integration validates the full stack.

### Phase 1: Type Foundation

Define all core types without implementation:
- Command enum with all 101 variants
- Output enum with all ~40 variants
- Error enum with all ~25 variants
- Supporting types (VersionedValue, VectorMatch, RunInfo, TxnInfo, etc.)

**Exit Criteria**: All types compile. All types serialize/deserialize. No implementation yet.

### Phase 2: Serialization

Implement JSON encoding/decoding:
- Value JSON encoding with special wrappers
- Output JSON encoding
- Error JSON encoding
- Round-trip tests passing

**Exit Criteria**: All types survive JSON round-trip. Special values handled correctly.

### Phase 3: Executor Core

Implement the executor:
- Executor struct and constructor
- Match dispatch for all 101 commands
- Error conversion layer
- All handlers implemented (9 handler modules)

**Exit Criteria**: All commands execute correctly. Parity with direct Substrate calls.

### Phase 4: Integration (M13 Exit Gate)

Final integration and validation:
- Workspace integration
- Comprehensive test coverage
- Determinism verification
- Performance validation

**Exit Criteria**: All tests pass. Ready for API layer integration.

### Phase Summary

| Phase | Epics | Key Deliverable | Status |
|-------|-------|-----------------|--------|
| 1 | 90, 91 | Type definitions | Pending |
| 2 | 93 | JSON serialization | Pending |
| 3 | 92 | Executor implementation | Pending |
| 4 | 94 | Integration & testing | Pending |

---

## Testing Strategy

### Unit Tests

- Command variant construction (all 101)
- Command field validation
- Output variant construction (all ~40)
- Error variant construction (all ~25)
- JSON encoding for each Value type
- Special wrapper encoding ($bytes, $f64)
- Error conversion from internal errors

### Integration Tests

- Full command execution flow
- Multi-command sequences
- Run-scoped operations
- Cross-primitive workflows
- Error propagation through executor
- Transaction begin/commit/rollback flows
- Retention policy operations

### Parity Tests

- Every command vs direct Substrate call
- Same inputs produce same outputs
- Same errors for same invalid inputs
- Version numbers match
- Timestamps in expected range
- All 8 primitive categories covered

### Round-Trip Tests

- All 101 command variants through JSON
- All ~40 output variants through JSON
- All ~25 error variants through JSON
- Special float values (NaN, Inf, -0.0)
- Binary data (bytes)
- Large integers (i64 boundaries)
- Unicode strings
- Nested objects and arrays
- Vector embeddings (f32 arrays)
- SearchFilter serialization

### Determinism Tests

- Same command sequence produces same state
- Order matters (verify ordering)
- Repeated execution produces same results
- No time-dependent behavior in commands

### Performance Tests

- Command dispatch overhead (target: <100ns)
- Serialization overhead (target: <1μs per command)
- No allocation in hot path
- Memory usage stable

---

## Success Metrics

**Functional**: All ~40 stories passing, 100% acceptance criteria met

**Type Coverage**:
- All 101 command variants implemented
- All ~40 output variants implemented
- All ~25 error variants implemented
- No Generic/Any fallbacks

**API Parity**:
- Every Substrate API operation has a corresponding Command
- Executor execution produces identical results to direct Substrate calls
- Error behavior matches Substrate error behavior
- All 8 primitive categories covered (KV, JSON, Event, State, Vector, Run, Transaction, Retention)

**Serialization**:
- 100% round-trip accuracy
- All special values preserved
- No precision loss

**Performance**:
- Dispatch overhead < 100ns
- No measurable impact on primitive performance

**Quality**: Test coverage > 95% for executor crate

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Missing command variants | Low | High | Systematic audit against primitives |
| Type mismatch in Output | Medium | Medium | Comprehensive type mapping tests |
| Serialization precision loss | Medium | High | Extensive round-trip tests, special wrappers |
| Performance overhead | Low | Medium | Benchmark early, optimize dispatch |
| Error information loss | Medium | Medium | Error conversion tests, detail preservation |
| Breaking existing code | Low | High | Executor is additive, old API unchanged |

---

## Not In Scope (Explicitly Deferred)

1. **Async execution** - Post-MVP (Commands are synchronous)
2. **Transaction batching** - Post-MVP (`execute_atomic` is placeholder)
3. **Command middleware** - Post-MVP (logging, metrics)
4. **Command replay infrastructure** - Post-MVP (logging, replay CLI)
5. **Remote execution** - Post-MVP (server integration)
6. **Command versioning** - Post-MVP (for wire protocol evolution)
7. **Batch optimization** - Post-MVP (mget/mput optimization)
8. **Streaming results** - Post-MVP (large result sets)

---

## Post-M13 Expectations

After M13 completion:
1. Every Strata operation expressible as a typed Command
2. Commands are self-contained and serializable
3. Executor provides single entry point to all primitives
4. JSON encoding handles all edge cases (special floats, bytes)
5. Black-box testing enabled (feed commands, assert results)
6. Deterministic replay possible (same commands = same state)
7. Foundation ready for thin SDKs (Python, Node, CLI)
8. No performance regression from command abstraction
9. Wire protocol (future M14) has clean command interface to build on
10. RunBundle integration straightforward (commands as semantic log)

---

## Command Count Summary

| Category | Commands | API Trait |
|----------|----------|-----------|
| KV | 15 | `KVStore`, `KVStoreBatch` |
| JSON | 17 | `JsonStore` |
| Events | 11 | `EventLog` |
| State | 8 | `StateCell` (excludes 3 closure-based) |
| Vectors | 19 | `VectorStore` |
| Runs | 24 | `RunIndex` |
| Transaction | 5 | `TransactionControl` |
| Retention | 3 | `RetentionSubstrate` |
| Database | 4 | *(internal)* |
| **Total** | **106** | |

**Note**: 5 methods are excluded as they require closures:
- `state_transition` (closure)
- `state_transition_or_init` (closure)
- `state_get_or_init` (closure)

This leaves **101 implementable command variants** plus 5 deferred.

### Command Variant Details

**KV (15)**:
`KvPut`, `KvGet`, `KvGetAt`, `KvDelete`, `KvExists`, `KvHistory`, `KvIncr`, `KvCasVersion`, `KvCasValue`, `KvKeys`, `KvScan`, `KvMget`, `KvMput`, `KvMdelete`, `KvMexists`

**JSON (17)**:
`JsonSet`, `JsonGet`, `JsonDelete`, `JsonMerge`, `JsonHistory`, `JsonExists`, `JsonGetVersion`, `JsonSearch`, `JsonList`, `JsonCas`, `JsonQuery`, `JsonCount`, `JsonBatchGet`, `JsonBatchCreate`, `JsonArrayPush`, `JsonIncrement`, `JsonArrayPop`

**Event (11)**:
`EventAppend`, `EventAppendBatch`, `EventRange`, `EventGet`, `EventLen`, `EventLatestSequence`, `EventStreamInfo`, `EventRevRange`, `EventStreams`, `EventHead`, `EventVerifyChain`

**State (8)**:
`StateSet`, `StateGet`, `StateCas`, `StateDelete`, `StateExists`, `StateHistory`, `StateInit`, `StateList`

**Vector (19)**:
`VectorUpsert`, `VectorUpsertWithSource`, `VectorGet`, `VectorDelete`, `VectorSearch`, `VectorSearchWithBudget`, `VectorCollectionInfo`, `VectorCreateCollection`, `VectorDropCollection`, `VectorListCollections`, `VectorCollectionExists`, `VectorCount`, `VectorUpsertBatch`, `VectorGetBatch`, `VectorDeleteBatch`, `VectorHistory`, `VectorGetAt`, `VectorListKeys`, `VectorScan`

**Run (24)**:
`RunCreate`, `RunGet`, `RunList`, `RunClose`, `RunUpdateMetadata`, `RunExists`, `RunPause`, `RunResume`, `RunFail`, `RunCancel`, `RunArchive`, `RunDelete`, `RunQueryByStatus`, `RunQueryByTag`, `RunCount`, `RunSearch`, `RunAddTags`, `RunRemoveTags`, `RunGetTags`, `RunCreateChild`, `RunGetChildren`, `RunGetParent`, `RunSetRetention`, `RunGetRetention`

**Transaction (5)**:
`TxnBegin`, `TxnCommit`, `TxnRollback`, `TxnInfo`, `TxnIsActive`

**Retention (3)**:
`RetentionGet`, `RetentionSet`, `RetentionClear`

**Database (4)**:
`Ping`, `Info`, `Flush`, `Compact`

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-25 | Initial M13 implementation plan |
| 1.1 | 2026-01-25 | Expanded to cover full API surface (101 commands vs original 48) |

---

**This is the implementation plan. All work must conform to it.**
