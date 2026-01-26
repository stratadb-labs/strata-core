# M13 Architecture: Command Execution Layer

**Version**: 1.0
**Status**: Implementation Ready
**Last Updated**: 2026-01-25

---

## Executive Summary

This document specifies the architecture for **Milestone 13 (M13): Command Execution Layer** of the Strata database. M13 introduces a standardized command interface between all external APIs (Rust, Python, CLI, MCP) and the core database engine.

**THIS DOCUMENT IS AUTHORITATIVE.** All M13 implementation must conform to this specification.

**M13 Philosophy**:
> Every mutation, read, or lifecycle operation in Strata must be representable as a typed, serializable `Command`. Commands are the "instruction set" of Strata. If something cannot be expressed as a command, it is not part of Strata's public behavior.
>
> The Command Execution Layer is an in-process execution boundary, not a wire protocol. It provides a stable, language-agnostic execution model that enables deterministic replay, thin SDKs, and black-box testing.

**Critical Architectural Position**:
> **M13 is an abstraction milestone, not a feature milestone.** It wraps existing primitive operations in a uniform command interface without changing any primitive behavior. The executor dispatches commands to primitives—it does not duplicate primitive logic or enforce semantic invariants.

**M13 Goals**:
- Define a complete command set covering all 101 primitive operations
- Provide typed, serializable Commands, Outputs, and Errors
- Enable thin SDKs that construct commands and interpret results
- Support deterministic replay for crash recovery and RunBundles
- Enable black-box testing at the command level
- Establish canonical JSON encoding for wire transport

**M13 Non-Goals** (Deferred):
- Async execution
- Transaction batching (`execute_atomic`)
- Command middleware (logging, metrics)
- Remote execution / wire protocol
- Command versioning tags
- Streaming results

**Critical Constraint**:
> M13 adds an abstraction layer. It does not change semantic behavior. If a change affects user-visible behavior beyond providing a command interface, it is out of scope.

**Built on M1-M11**:
- M1-M9: Seven primitives (KV, JSON, Event, State, Vector, Run, Trace)
- M10: Storage backend, WAL, snapshots, retention, compaction
- M11: Public API contract, Substrate/Facade model, value model, wire encoding
- M13 adds: Command execution layer, typed command interface, thin SDK enablement

---

## Table of Contents

1. [Scope Boundaries](#1-scope-boundaries)
2. [THE SEVEN ARCHITECTURAL RULES](#2-the-seven-architectural-rules-non-negotiable)
3. [Core Invariants](#3-core-invariants)
4. [Command Execution Model](#4-command-execution-model)
5. [Command Types](#5-command-types)
6. [Output Types](#6-output-types)
7. [Error Model](#7-error-model)
8. [Executor Contract](#8-executor-contract)
9. [Serialization Contract](#9-serialization-contract)
10. [Migration Strategy](#10-migration-strategy)
11. [Client-Side Patterns](#11-client-side-patterns)
12. [SDK Contract](#12-sdk-contract)
13. [Testing Strategy](#13-testing-strategy)
14. [Known Limitations](#14-known-limitations)
15. [Future Extension Points](#15-future-extension-points)
16. [Success Criteria Checklist](#16-success-criteria-checklist)

---

## 1. Scope Boundaries

### 1.1 What M13 IS

M13 is an **abstraction milestone**. It defines:

| Aspect | M13 Commits To |
|--------|----------------|
| **Command Types** | 101 typed command variants covering all primitive operations |
| **Output Types** | ~40 typed output variants for all return types |
| **Error Types** | ~25 typed error variants with structured details |
| **Executor** | Single dispatch point from Command to primitive calls |
| **Serialization** | Lossless JSON round-trip for all types |
| **Client Patterns** | Blessed helpers for closure-equivalent operations |

### 1.2 What M13 is NOT

M13 is **not** a feature milestone. These are explicitly deferred:

| Deferred Item | Why Deferred | Target |
|---------------|--------------|--------|
| Async execution | Complexity | Post-MVP |
| Transaction batching | Complexity | Post-MVP |
| Command middleware | Scope | Post-MVP |
| Wire protocol | Scope | M14+ |
| Command versioning | YAGNI | If needed |
| Remote execution | Scope | M14+ |
| Streaming results | Complexity | Post-MVP |

### 1.3 The Risk We Are Avoiding

Without a command execution layer:
- Each SDK reimplements invariant enforcement
- Bugs drift between SDK implementations
- Semantic divergence across surfaces (Rust, Python, CLI)
- No deterministic replay capability
- RunBundle integration becomes ad-hoc
- Black-box testing requires coupling to internal traits

**M13 provides the canonical execution boundary.** All future SDKs and wire protocols build on commands.

### 1.4 Evolution Warnings

**These are explicit warnings about M13 design decisions:**

#### A. Commands Are Data, Not Code

Commands are pure data structures. They cannot contain closures, function pointers, or executable code.

**Rationale**: Commands must serialize for replay, logging, and thin SDK support. Executable code cannot be serialized.

**Implication**: Substrate methods that take closures (`state_transition`) have no direct command equivalent. Clients compose commands instead.

#### B. M13 Is Additive

M13 does NOT delete strata-api. The executor exists alongside the existing API layer.

**Rationale**: Turning an abstraction milestone into a big-bang migration is where schedules die. Two-step landing allows validation before commitment.

#### C. Executor Does Not Enforce Semantic Invariants

The executor is a dispatch layer. It routes commands to primitives. Semantic invariants (run scoping, isolation, version correctness) are enforced by primitives/engine.

**Rationale**: Duplicating primitive logic in the executor creates two sources of truth. The executor should be a thin veneer.

#### D. SDK Helpers Are Not Commands

Ergonomic compositions (retry loops, get-or-init) are SDK helpers, not commands. Commands are the minimal semantic core.

**Rationale**: Unbounded command growth leads to an unmaintainable API. SDK helpers provide ergonomics without expanding the command set.

---

## 2. THE SEVEN ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in ALL M13 implementation. Violating any of these is a blocking issue.**

### Rule 1: Commands Are Complete

> **Every public primitive operation MUST have a corresponding Command variant.**

```rust
// CORRECT: Complete coverage
pub enum Command {
    KvPut { run: RunId, key: String, value: Value },
    KvGet { run: RunId, key: String },
    // ... all 101 variants
}

// WRONG: Missing operations
pub enum Command {
    KvPut { ... },
    // kv_get missing!
}
```

**Why**: If an operation cannot be expressed as a command, it is not part of Strata's public behavior.

### Rule 2: Commands Are Self-Contained

> **Every Command variant contains all information needed to execute. No implicit context.**

```rust
// CORRECT: All context explicit
Command::KvPut {
    run: RunId::from("my-run"),
    key: "foo".into(),
    value: Value::Int(42),
}

// WRONG: Implicit run context
Command::KvPut {
    key: "foo".into(),  // Where's the run?
    value: Value::Int(42),
}
```

**Why**: Commands must be replayable. Replay requires all context to be in the command.

### Rule 3: Executor Is Stateless

> **The Executor dispatches commands to primitives. It holds references to primitives but maintains no state of its own.**

```rust
// CORRECT: Stateless executor
pub struct Executor {
    substrate: Arc<SubstrateImpl>,
}

impl Executor {
    pub fn execute(&self, cmd: Command) -> Result<Output, Error> {
        match cmd {
            Command::KvPut { run, key, value } => {
                self.substrate.kv_put(&run.into(), &key, value)
                    .map(|v| Output::Version(v))
                    .map_err(Error::from)
            }
            // ...
        }
    }
}

// WRONG: Executor with state
pub struct Executor {
    substrate: Arc<SubstrateImpl>,
    command_count: AtomicU64,  // NO! Executor state
    cache: HashMap<String, Value>,  // NO! Executor state
}
```

**Why**: All state lives in the engine. Executor state would break replay semantics.

### Rule 4: Output Matches Command Deterministically

> **Each Command variant has a deterministic Output type. Same Command on same state = same Output.**

```rust
// CORRECT: Deterministic mapping
Command::KvPut { ... }  -> Output::Version(v)
Command::KvGet { ... }  -> Output::MaybeVersioned(Option<VersionedValue>)
Command::KvExists { ... } -> Output::Bool(bool)

// WRONG: Non-deterministic output
fn execute(&self, cmd: Command) -> Output {
    match cmd {
        Command::KvGet { .. } => {
            if rand::random() {
                Output::Value(...)  // NO! Non-deterministic
            } else {
                Output::MaybeVersioned(...)
            }
        }
    }
}
```

**Why**: Determinism underpins crash recovery, replay, and testing.

### Rule 5: Errors Are Structured

> **All errors are represented by the Error enum. No panics, no string-only errors, no error swallowing.**

```rust
// CORRECT: Structured error
pub enum Error {
    KeyNotFound { key: String },
    DimensionMismatch { expected: usize, actual: usize },
}

// WRONG: String-only error
pub enum Error {
    Generic(String),  // Lost structure!
}

// WRONG: Panic
fn execute(&self, cmd: Command) -> Result<Output, Error> {
    panic!("unexpected command");  // NO!
}

// WRONG: Swallowing
fn execute(&self, cmd: Command) -> Result<Output, Error> {
    self.substrate.kv_get(...).ok();  // NO! Error swallowed
}
```

**Why**: Structured errors enable proper error handling in SDKs.

### Rule 6: Serialization Is Lossless

> **Commands, Outputs, and Errors MUST serialize and deserialize without loss. Round-trip must be exact.**

```rust
// CORRECT: Exact round-trip
let cmd = Command::KvPut { run, key, value: Value::Float(f64::NAN) };
let json = serde_json::to_string(&cmd)?;
let restored: Command = serde_json::from_str(&json)?;
assert_eq!(cmd, restored);  // Including NaN!

// WRONG: Precision loss
let cmd = Command::VectorInsert { embedding: vec![0.1f32, 0.2, 0.3], ... };
let json = serde_json::to_string(&cmd)?;
let restored: Command = serde_json::from_str(&json)?;
// embedding[0] is now 0.10000000149... due to f32->f64->f32
```

**Why**: Lossy serialization breaks replay and client expectations.

### Rule 7: No Executable Code in Commands

> **Commands are pure data. They cannot contain closures, function pointers, or any executable code.**

```rust
// CORRECT: Pure data
Command::StateCas {
    run: run.clone(),
    cell: "counter".into(),
    expected_counter: Some(5),
    value: Value::Int(6),
}

// WRONG: Closure in command
Command::StateTransition {
    run: run.clone(),
    cell: "counter".into(),
    transform: |v| Value::Int(v.as_int().unwrap() + 1),  // NO!
}
```

**Why**: Closures cannot be serialized. Security risk from deserializing executable code.

---

## 3. Core Invariants

### 3.1 Command Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| CMD-1 | Every primitive operation has a Command variant | 101 commands cover all operations |
| CMD-2 | Commands are self-contained | No external context required |
| CMD-3 | Commands serialize/deserialize losslessly | JSON round-trip exact |
| CMD-4 | Command execution is deterministic | Same command + same state = same result |
| CMD-5 | All commands are typed | No `Generic(Value)` fallback |
| CMD-6 | Commands are pure data | No closures, no function pointers |

### 3.2 Output Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| OUT-1 | Output variants cover all return types | ~40 variants for all returns |
| OUT-2 | Outputs serialize/deserialize losslessly | JSON round-trip exact |
| OUT-3 | Output matches expected type for each Command | Deterministic type mapping |
| OUT-4 | Versioned outputs preserve version metadata | Version, timestamp intact |

### 3.3 Error Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| ERR-1 | All primitive errors map to Error variants | No information lost |
| ERR-2 | Errors serialize/deserialize losslessly | JSON round-trip exact |
| ERR-3 | Errors include structured details | Not just strings |
| ERR-4 | No error swallowing or transformation | Errors propagate faithfully |
| ERR-5 | No panics in command execution | Result type for all errors |

### 3.4 Executor Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| EXE-1 | Executor is stateless | No command-to-command state |
| EXE-2 | execute() dispatches correctly to all 101 variants | Complete coverage |
| EXE-3 | execute_many() processes sequentially | Order preserved |
| EXE-4 | Executor does not modify command semantics | Pure dispatch |
| EXE-5 | Executor is Send + Sync | Thread-safe |

### 3.5 Serialization Invariants

| # | Invariant | Meaning |
|---|-----------|---------|
| SER-1 | JSON encoding handles all Value types | 8 types covered |
| SER-2 | Special values preserved | `$bytes`, `$f64` wrappers |
| SER-3 | Large integers preserved | Full i64 range |
| SER-4 | Binary data encoded as base64 | Via `$bytes` wrapper |
| SER-5 | Canonical JSON encoding | Deterministic serialization |

### 3.6 Invariant Enforcement Boundary

| Layer | Enforces |
|-------|----------|
| **Executor** | Commands self-contained, typed, serializable, deterministic output mapping, no panics |
| **Primitives/Engine** | Run scoping, isolation, version correctness, retention rules, constraint violations |

**The executor does NOT duplicate primitive validation.**

---

## 4. Command Execution Model

### 4.1 Architecture Overview

```
Rust SDK     Python SDK     CLI     MCP Server
     │            │          │           │
     └────────────┴──────────┴───────────┘
                       │
          ┌────────────┴────────────┐
          │     Command (enum)      │  ← Typed, serializable
          │     101 variants        │
          └────────────┬────────────┘
                       │
          ┌────────────┴────────────┐
          │       Executor          │  ← Stateless dispatch
          │   execute(cmd) -> Result│
          └────────────┬────────────┘
                       │
          ┌────────────┴────────────┐
          │     Output (enum)       │  ← Typed results
          │     ~40 variants        │
          └────────────┬────────────┘
                       │
                       ▼
          ┌─────────────────────────┐
          │   SubstrateImpl         │  ← Existing implementation
          │   (KV, JSON, Event,     │
          │    State, Vector, Run)  │
          └─────────────────────────┘
```

### 4.2 Execution Flow

1. **Client constructs Command** - Typed enum variant with all parameters
2. **Command serialized (optional)** - For Python/MCP, command becomes JSON
3. **Executor.execute(cmd) called** - Dispatch to appropriate primitive
4. **Primitive executes** - Real work happens in engine
5. **Result converted to Output** - Primitive return → Output variant
6. **Output serialized (optional)** - For Python/MCP, output becomes JSON
7. **Client interprets Output** - SDK helper extracts typed value

### 4.3 Example: KV Put

```rust
// 1. Client constructs command
let cmd = Command::KvPut {
    run: RunId::from("default"),
    key: "user:123".into(),
    value: Value::Object(user_map),
};

// 2. JSON for Python client (optional)
// {"KvPut":{"run":"default","key":"user:123","value":{...}}}

// 3. Executor dispatches
let output = executor.execute(cmd)?;

// 4. Primitive executes
// substrate.kv_put(&run, "user:123", value) internally

// 5. Output returned
// Output::Version(Version::Txn(42))

// 6. JSON for Python client (optional)
// {"Version":{"Txn":42}}

// 7. Client interprets
// Python: result["Version"]["Txn"] -> 42
```

---

## 5. Command Types

### 5.1 Command Enum Structure

```rust
/// A command is a self-contained, serializable operation.
/// This is the "instruction set" of Strata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    // ==================== KV (15) ====================
    KvPut { run: RunId, key: String, value: Value },
    KvGet { run: RunId, key: String },
    KvGetAt { run: RunId, key: String, version: u64 },
    KvDelete { run: RunId, key: String },
    KvExists { run: RunId, key: String },
    KvHistory { run: RunId, key: String, limit: Option<u64>, before: Option<u64> },
    KvIncr { run: RunId, key: String, delta: i64 },
    KvCasVersion { run: RunId, key: String, expected_version: Option<u64>, new_value: Value },
    KvCasValue { run: RunId, key: String, expected_value: Option<Value>, new_value: Value },
    KvKeys { run: RunId, prefix: String, limit: Option<u64> },
    KvScan { run: RunId, prefix: String, limit: u64, cursor: Option<String> },
    KvMget { run: RunId, keys: Vec<String> },
    KvMput { run: RunId, entries: Vec<(String, Value)> },
    KvMdelete { run: RunId, keys: Vec<String> },
    KvMexists { run: RunId, keys: Vec<String> },

    // ==================== JSON (17) ====================
    JsonSet { run: RunId, key: String, path: String, value: Value },
    JsonGet { run: RunId, key: String, path: String },
    JsonDelete { run: RunId, key: String, path: String },
    JsonMerge { run: RunId, key: String, path: String, patch: Value },
    JsonHistory { run: RunId, key: String, limit: Option<u64>, before: Option<u64> },
    JsonExists { run: RunId, key: String },
    JsonGetVersion { run: RunId, key: String },
    JsonSearch { run: RunId, query: String, k: usize },
    JsonList { run: RunId, prefix: Option<String>, cursor: Option<String>, limit: u64 },
    JsonCas { run: RunId, key: String, expected_version: u64, path: String, value: Value },
    JsonQuery { run: RunId, path: String, value: Value, limit: u64 },
    JsonCount { run: RunId },
    JsonBatchGet { run: RunId, keys: Vec<String> },
    JsonBatchCreate { run: RunId, docs: Vec<(String, Value)> },
    JsonArrayPush { run: RunId, key: String, path: String, values: Vec<Value> },
    JsonIncrement { run: RunId, key: String, path: String, delta: f64 },
    JsonArrayPop { run: RunId, key: String, path: String },

    // ==================== Events (11) ====================
    EventAppend { run: RunId, stream: String, payload: Value },
    EventAppendBatch { run: RunId, events: Vec<(String, Value)> },
    EventRange { run: RunId, stream: String, start: Option<u64>, end: Option<u64>, limit: Option<u64> },
    EventGet { run: RunId, stream: String, sequence: u64 },
    EventLen { run: RunId, stream: String },
    EventLatestSequence { run: RunId, stream: String },
    EventStreamInfo { run: RunId, stream: String },
    EventRevRange { run: RunId, stream: String, start: Option<u64>, end: Option<u64>, limit: Option<u64> },
    EventStreams { run: RunId },
    EventHead { run: RunId, stream: String },
    EventVerifyChain { run: RunId },

    // ==================== State (8) ====================
    StateSet { run: RunId, cell: String, value: Value },
    StateGet { run: RunId, cell: String },
    StateCas { run: RunId, cell: String, expected_counter: Option<u64>, value: Value },
    StateDelete { run: RunId, cell: String },
    StateExists { run: RunId, cell: String },
    StateHistory { run: RunId, cell: String, limit: Option<u64>, before: Option<u64> },
    StateInit { run: RunId, cell: String, value: Value },
    StateList { run: RunId },

    // ==================== Vectors (19) ====================
    VectorUpsert { run: RunId, collection: String, key: String, vector: Vec<f32>, metadata: Option<Value> },
    VectorUpsertWithSource { run: RunId, collection: String, key: String, vector: Vec<f32>, metadata: Option<Value>, source_ref: Option<String> },
    VectorGet { run: RunId, collection: String, key: String },
    VectorDelete { run: RunId, collection: String, key: String },
    VectorSearch { run: RunId, collection: String, query: Vec<f32>, k: usize, filter: Option<MetadataFilter>, metric: Option<DistanceMetric> },
    VectorSearchWithBudget { run: RunId, collection: String, query: Vec<f32>, k: usize, filter: Option<MetadataFilter>, budget: usize },
    VectorCollectionInfo { run: RunId, collection: String },
    VectorCreateCollection { run: RunId, collection: String, dimension: usize, metric: DistanceMetric },
    VectorDropCollection { run: RunId, collection: String },
    VectorListCollections { run: RunId },
    VectorCollectionExists { run: RunId, collection: String },
    VectorCount { run: RunId, collection: String },
    VectorUpsertBatch { run: RunId, collection: String, vectors: Vec<VectorEntry> },
    VectorGetBatch { run: RunId, collection: String, keys: Vec<String> },
    VectorDeleteBatch { run: RunId, collection: String, keys: Vec<String> },
    VectorHistory { run: RunId, collection: String, key: String, limit: Option<u64>, before_version: Option<u64> },
    VectorGetAt { run: RunId, collection: String, key: String, version: u64 },
    VectorListKeys { run: RunId, collection: String, limit: Option<u64>, cursor: Option<String> },
    VectorScan { run: RunId, collection: String, limit: Option<u64>, cursor: Option<String> },

    // ==================== Runs (24) ====================
    RunCreate { run_id: Option<String>, metadata: Option<Value> },
    RunGet { run: RunId },
    RunList { state: Option<RunStatus>, limit: Option<u64>, offset: Option<u64> },
    RunClose { run: RunId },
    RunUpdateMetadata { run: RunId, metadata: Value },
    RunExists { run: RunId },
    RunPause { run: RunId },
    RunResume { run: RunId },
    RunFail { run: RunId, error: String },
    RunCancel { run: RunId },
    RunArchive { run: RunId },
    RunDelete { run: RunId },
    RunQueryByStatus { state: RunStatus },
    RunQueryByTag { tag: String },
    RunCount { status: Option<RunStatus> },
    RunSearch { query: String, limit: Option<u64> },
    RunAddTags { run: RunId, tags: Vec<String> },
    RunRemoveTags { run: RunId, tags: Vec<String> },
    RunGetTags { run: RunId },
    RunCreateChild { parent: RunId, metadata: Option<Value> },
    RunGetChildren { parent: RunId },
    RunGetParent { run: RunId },
    RunSetRetention { run: RunId, policy: RetentionPolicy },
    RunGetRetention { run: RunId },

    // ==================== Transaction (5) ====================
    TxnBegin { options: Option<TxnOptions> },
    TxnCommit,
    TxnRollback,
    TxnInfo,
    TxnIsActive,

    // ==================== Retention (3) ====================
    RetentionGet { run: RunId },
    RetentionSet { run: RunId, policy: RetentionPolicy },
    RetentionClear { run: RunId },

    // ==================== Database (4) ====================
    Ping,
    Info,
    Flush,
    Compact,
}
```

### 5.2 Command Count Summary

| Category | Count | API Trait |
|----------|-------|-----------|
| KV | 15 | `KVStore`, `KVStoreBatch` |
| JSON | 17 | `JsonStore` |
| Events | 11 | `EventLog` |
| State | 8 | `StateCell` (excludes closures) |
| Vectors | 19 | `VectorStore` |
| Runs | 24 | `RunIndex` |
| Transaction | 5 | `TransactionControl` |
| Retention | 3 | `RetentionSubstrate` |
| Database | 4 | *(internal)* |
| **Total** | **106** | |

**Note**: 5 methods are excluded as they require closures (not serializable).

### 5.3 Excluded Methods (Closure-Based)

| Method | Reason |
|--------|--------|
| `state_transition` | Takes `Fn(&Value) -> Result<Value>` |
| `state_transition_or_init` | Takes `Fn(&Value) -> Result<Value>` |
| `state_get_or_init` | Takes `Fn() -> Value` |

These are implemented as client-side patterns (see Section 11).

---

## 6. Output Types

### 6.1 Output Enum Structure

```rust
/// Successful command outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Output {
    // Primitive results
    Unit,
    Value(Value),
    Versioned(VersionedValue),
    Maybe(Option<Value>),
    MaybeVersioned(Option<VersionedValue>),
    MaybeVersion(Option<u64>),
    Version(Version),
    Bool(bool),
    Int(i64),
    Uint(u64),
    Float(f64),

    // Collections
    Values(Vec<Option<VersionedValue>>),
    VersionedValues(Vec<VersionedValue>),
    Versions(Vec<Version>),
    Keys(Vec<String>),
    Strings(Vec<String>),
    Bools(Vec<bool>),

    // KV-specific
    KVScanResult { entries: Vec<(String, VersionedValue)>, cursor: Option<String> },

    // JSON-specific
    JsonListResult { keys: Vec<String>, cursor: Option<String> },
    JsonSearchHits(Vec<JsonSearchHit>),

    // Event-specific
    StreamInfo(StreamInfo),
    ChainVerification(ChainVerification),

    // Vector-specific
    VectorData(Option<VersionedVectorData>),
    VectorDataList(Vec<Option<VersionedVectorData>>),
    VectorDataHistory(Vec<VersionedVectorData>),
    VectorMatches(Vec<VectorMatch>),
    VectorMatchesWithExhausted((Vec<VectorMatch>, bool)),
    VectorKeyValues(Vec<(String, VectorData)>),
    VectorBatchResult(Vec<Result<(String, Version), Error>>),
    VectorCollectionInfo(Option<CollectionInfo>),
    VectorCollectionList(Vec<CollectionInfo>),

    // Run-specific
    RunInfo(RunInfo),
    RunInfoVersioned(VersionedRunInfo),
    RunInfoList(Vec<VersionedRunInfo>),
    RunWithVersion((RunInfo, Version)),
    MaybeRunId(Option<RunId>),

    // Transaction-specific
    TxnId(TxnId),
    TxnInfo(Option<TxnInfo>),

    // Retention-specific
    RetentionVersion(Option<RetentionVersion>),
    RetentionPolicy(RetentionPolicy),

    // Database-specific
    DatabaseInfo(DatabaseInfo),
    Pong { version: String },
}
```

### 6.2 Command-Output Mapping

Every command has exactly one output type. This mapping is deterministic.

| Command Category | Typical Output Types |
|------------------|---------------------|
| Writes (put, set, upsert) | `Version` |
| Reads (get) | `MaybeVersioned` |
| Point-in-time reads (get_at) | `Versioned` |
| Existence checks | `Bool` |
| Deletes | `Bool` or `Uint` (count) |
| History queries | `VersionedValues` |
| Batch reads | `Values` |
| Search operations | Specialized result types |

---

## 7. Error Model

### 7.1 Error Enum Structure

```rust
/// Command execution errors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
pub enum Error {
    // Not Found errors
    #[error("key not found: {key}")]
    KeyNotFound { key: String },

    #[error("run not found: {run}")]
    RunNotFound { run: String },

    #[error("collection not found: {collection}")]
    CollectionNotFound { collection: String },

    #[error("stream not found: {stream}")]
    StreamNotFound { stream: String },

    #[error("cell not found: {cell}")]
    CellNotFound { cell: String },

    #[error("document not found: {key}")]
    DocumentNotFound { key: String },

    // Type errors
    #[error("wrong type: expected {expected}, got {actual}")]
    WrongType { expected: String, actual: String },

    // Validation errors
    #[error("invalid key: {reason}")]
    InvalidKey { reason: String },

    #[error("invalid path: {reason}")]
    InvalidPath { reason: String },

    #[error("invalid input: {reason}")]
    InvalidInput { reason: String },

    // Concurrency errors
    #[error("version conflict: expected {expected}, got {actual}")]
    VersionConflict { expected: u64, actual: u64 },

    #[error("transition failed: expected {expected}, got {actual}")]
    TransitionFailed { expected: String, actual: String },

    #[error("conflict: {reason}")]
    Conflict { reason: String },

    // State errors
    #[error("run closed: {run}")]
    RunClosed { run: String },

    #[error("run already exists: {run}")]
    RunExists { run: String },

    #[error("collection already exists: {collection}")]
    CollectionExists { collection: String },

    // Constraint errors
    #[error("dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("constraint violation: {reason}")]
    ConstraintViolation { reason: String },

    #[error("history trimmed: requested {requested}, earliest is {earliest}")]
    HistoryTrimmed { requested: u64, earliest: u64 },

    #[error("overflow: {reason}")]
    Overflow { reason: String },

    // Transaction errors
    #[error("no active transaction")]
    TransactionNotActive,

    #[error("transaction already active")]
    TransactionAlreadyActive,

    // System errors
    #[error("I/O error: {reason}")]
    Io { reason: String },

    #[error("serialization error: {reason}")]
    Serialization { reason: String },

    #[error("internal error: {reason}")]
    Internal { reason: String },
}
```

### 7.2 Error Categories

| Category | Errors | Typical Cause |
|----------|--------|---------------|
| Not Found | `KeyNotFound`, `RunNotFound`, `CollectionNotFound`, `StreamNotFound`, `CellNotFound`, `DocumentNotFound` | Missing entity |
| Type | `WrongType` | Type mismatch |
| Validation | `InvalidKey`, `InvalidPath`, `InvalidInput` | Bad input |
| Concurrency | `VersionConflict`, `TransitionFailed`, `Conflict` | Race condition |
| State | `RunClosed`, `RunExists`, `CollectionExists` | Invalid state transition |
| Constraint | `DimensionMismatch`, `ConstraintViolation`, `HistoryTrimmed`, `Overflow` | Limit exceeded |
| Transaction | `TransactionNotActive`, `TransactionAlreadyActive` | Transaction state |
| System | `Io`, `Serialization`, `Internal` | Infrastructure |

### 7.3 JSON Error Format

```json
{
  "error": "KeyNotFound",
  "key": "missing_key"
}
```

---

## 8. Executor Contract

### 8.1 Executor Structure

```rust
/// The command executor - single entry point to Strata's engine.
pub struct Executor {
    substrate: Arc<SubstrateImpl>,
}

impl Executor {
    /// Create a new executor wrapping a database substrate.
    pub fn new(substrate: Arc<SubstrateImpl>) -> Self {
        Self { substrate }
    }

    /// Execute a single command.
    pub fn execute(&self, cmd: Command) -> Result<Output, Error> {
        match cmd {
            Command::KvPut { run, key, value } => self.handle_kv_put(run, key, value),
            Command::KvGet { run, key } => self.handle_kv_get(run, key),
            // ... all 101 variants
        }
    }

    /// Execute multiple commands sequentially.
    /// Returns all results; continues on error.
    pub fn execute_many(&self, cmds: Vec<Command>) -> Vec<Result<Output, Error>> {
        cmds.into_iter().map(|cmd| self.execute(cmd)).collect()
    }
}

// Executor is thread-safe
unsafe impl Send for Executor {}
unsafe impl Sync for Executor {}
```

### 8.2 Executor Guarantees

| Guarantee | Description |
|-----------|-------------|
| **Stateless** | No command-to-command state in executor |
| **Deterministic** | Same command + same state = same result |
| **Complete** | All 101 commands handled |
| **Thread-safe** | `Send + Sync` |
| **No panics** | All errors via `Result` |

### 8.3 Handler Pattern

Each handler follows the same pattern:

```rust
impl Executor {
    fn handle_kv_put(&self, run: RunId, key: String, value: Value) -> Result<Output, Error> {
        // 1. Convert command types to API types
        let api_run = ApiRunId::from(run);

        // 2. Call substrate method
        let version = self.substrate
            .kv_put(&api_run, &key, value)
            .map_err(Error::from)?;

        // 3. Convert result to Output
        Ok(Output::Version(version))
    }
}
```

---

## 9. Serialization Contract

### 9.1 Canonical JSON Encoding

For RunBundle hashing, diffing, and deterministic replay:

**Requirement**: Semantically identical Values MUST serialize to identical JSON.

| Rule | Example |
|------|---------|
| Object keys sorted | `{"a":1,"b":2}` not `{"b":2,"a":1}` |
| No trailing zeros | `1.5` not `1.50` |
| No whitespace | Minified output |
| Consistent null | `null` not `"null"` |

### 9.2 Special Value Wrappers

| Value | JSON Encoding |
|-------|---------------|
| `Bytes(b)` | `{"$bytes":"<base64>"}` |
| `Float(NaN)` | `{"$f64":"NaN"}` |
| `Float(+Inf)` | `{"$f64":"+Inf"}` |
| `Float(-Inf)` | `{"$f64":"-Inf"}` |
| `Float(-0.0)` | `{"$f64":"-0.0"}` |

### 9.3 Value Type Mapping

| Strata Value | JSON |
|--------------|------|
| `Null` | `null` |
| `Bool(b)` | `true` / `false` |
| `Int(n)` | number |
| `Float(f)` | number (finite) or `$f64` wrapper |
| `String(s)` | string |
| `Bytes(b)` | `{"$bytes":"<base64>"}` |
| `Array(a)` | array |
| `Object(o)` | object |

### 9.4 Version Compatibility

| Scenario | Behavior |
|----------|----------|
| Newer executor, older commands | **Must work** - backwards compatible |
| Older executor, newer commands | **Fail gracefully** - unknown variant error |

**Rule**: New command variants may be added. Existing variants are immutable once released.

---

## 10. Migration Strategy

### 10.1 Two-Step Landing

**M13 is additive.** The existing strata-api remains functional.

```
M13 (This Milestone):                M13.1/M14 (Future):
┌─────────────────────┐              ┌─────────────────────┐
│  Python/MCP/CLI     │              │  Python/MCP/CLI     │
└─────────┬───────────┘              └─────────┬───────────┘
          │                                    │
          ▼                                    ▼
┌─────────────────────┐              ┌─────────────────────┐
│  strata-api         │◄── EXISTS    │  strata-executor    │◄── SOLE API
│  (Substrate/Facade) │              │  (Commands)         │
└─────────┬───────────┘              └─────────────────────┘
          │                                    │
┌─────────┴───────────┐                        │
│  strata-executor    │◄── NEW                 │
│  (Commands)         │                        │
└─────────┬───────────┘                        │
          │                                    │
          ▼                                    ▼
┌─────────────────────┐              ┌─────────────────────┐
│  strata-engine      │              │  strata-engine      │
└─────────────────────┘              └─────────────────────┘
```

### 10.2 M13 Deliverables

- Executor crate (`strata-executor`)
- All 101 command variants
- All ~40 output variants
- All ~25 error variants
- Parity tests (executor vs substrate)
- JSON serialization utilities

### 10.3 M13.1/M14 Deliverables (Future)

- Delete `strata-api` crate
- Port all substrate tests to executor tests
- Python SDK built on executor
- MCP server built on executor
- CLI built on executor

### 10.4 Rationale

Turning an "abstraction milestone" into a "big bang migration" is where schedules die. Two-step landing allows:
- Validation of executor behavior against substrate
- Incremental migration of callers
- Rollback capability if issues found

---

## 11. Client-Side Patterns

Since closure-based methods cannot be commands, clients must compose commands. These patterns MUST be shipped as blessed helpers in every SDK.

### 11.1 Pattern: Optimistic State Transition

```python
def state_transition(executor, run, cell, transform_fn, max_retries=10):
    """
    Atomically transform a state cell value.
    Equivalent to substrate.state_transition().
    """
    for attempt in range(max_retries):
        # 1. Read current state
        result = executor.execute({"StateGet": {"run": run, "cell": cell}})

        if result is None:
            raise CellNotFoundError(cell)

        current_value = result["value"]
        current_counter = result["version"]["Counter"]

        # 2. Apply transformation (client-side)
        new_value = transform_fn(current_value)

        # 3. Attempt CAS
        cas_result = executor.execute({
            "StateCas": {
                "run": run,
                "cell": cell,
                "expected_counter": current_counter,
                "value": new_value
            }
        })

        if cas_result is not None:  # CAS succeeded
            return (new_value, cas_result)

        # 4. Exponential backoff with jitter
        time.sleep(0.001 * (2 ** attempt) + random.uniform(0, 0.001))

    raise ConflictError(f"Failed after {max_retries} attempts")
```

### 11.2 Pattern: Get Or Initialize

```python
def state_get_or_init(executor, run, cell, default_fn):
    """
    Get cell value, initializing with default if not exists.
    Equivalent to substrate.state_get_or_init().
    """
    result = executor.execute({"StateGet": {"run": run, "cell": cell}})

    if result is not None:
        return result

    # Cell doesn't exist - initialize
    default_value = default_fn()
    version = executor.execute({
        "StateInit": {"run": run, "cell": cell, "value": default_value}
    })

    return {"value": default_value, "version": version}
```

### 11.3 Pattern: Idempotent Write

```python
def idempotent_kv_put(executor, run, key, value, expected_version=None):
    """
    Put that can be safely replayed.
    """
    if expected_version is not None:
        return executor.execute({
            "KvCasVersion": {
                "run": run,
                "key": key,
                "expected_version": expected_version,
                "new_value": value
            }
        })
    else:
        return executor.execute({
            "KvPut": {"run": run, "key": key, "value": value}
        })
```

### 11.4 SDK Helper Requirements

Every SDK MUST ship these patterns:
- `state_transition(run, cell, transform_fn, max_retries=10)`
- `state_get_or_init(run, cell, default_fn)`
- Retry utilities with jitter

---

## 12. SDK Contract

### 12.1 Rust Typed Wrapper

Rust users should not be forced to work with Command/Output enums directly. Ship a zero-cost typed veneer:

```rust
pub struct Strata {
    executor: Executor,
}

impl Strata {
    pub fn kv_put(&self, run: &RunId, key: &str, value: Value) -> Result<Version, Error> {
        match self.executor.execute(Command::KvPut {
            run: run.clone(),
            key: key.into(),
            value,
        })? {
            Output::Version(v) => Ok(v),
            _ => unreachable!("KvPut always returns Version"),
        }
    }

    pub fn kv_get(&self, run: &RunId, key: &str) -> Result<Option<VersionedValue>, Error> {
        match self.executor.execute(Command::KvGet {
            run: run.clone(),
            key: key.into(),
        })? {
            Output::MaybeVersioned(v) => Ok(v),
            _ => unreachable!("KvGet always returns MaybeVersioned"),
        }
    }

    // ... all other methods
}
```

This wrapper:
- Compiles down to direct command construction (no JSON in Rust)
- Provides idiomatic Rust API with proper return types
- Hides the enum matching from normal users
- Still allows direct executor access for power users

### 12.2 Python SDK

```python
class Strata:
    def __init__(self, executor):
        self._executor = executor
        self._run = "default"

    def put(self, key: str, value: Any) -> int:
        result = self._executor.execute({
            "KvPut": {"run": self._run, "key": key, "value": value}
        })
        return result["Version"]["Txn"]

    def get(self, key: str) -> Optional[Any]:
        result = self._executor.execute({
            "KvGet": {"run": self._run, "key": key}
        })
        if result is None:
            return None
        return result["value"]
```

### 12.3 SDK Requirements

All SDKs MUST:
1. Construct typed Commands (or JSON equivalent)
2. Call `executor.execute()`
3. Interpret typed Outputs (or JSON equivalent)
4. Ship blessed helpers for closure-equivalent operations
5. Not reimplement primitive logic

---

## 13. Testing Strategy

### 13.1 M13 Test Focus

| Category | What's Tested | Priority |
|----------|---------------|----------|
| **Parity** | Executor produces same results as substrate | CRITICAL |
| **Serialization** | Commands/Outputs survive JSON round-trip | CRITICAL |
| **Determinism** | Same command + same state = same result | CRITICAL |
| **Error mapping** | Substrate errors map correctly | HIGH |

### 13.2 Parity Tests

For each of the 101 commands, one parity test:

```rust
#[test]
fn test_parity_kv_put() {
    let (_, substrate) = quick_setup();
    let executor = Executor::new(substrate.clone());
    let run = ApiRunId::default();

    // Direct substrate call
    let direct = substrate.kv_put(&run, "key", Value::Int(42)).unwrap();

    // Reset
    substrate.kv_delete(&run, "key").unwrap();

    // Executor call
    let exec = executor.execute(Command::KvPut {
        run: "default".into(),
        key: "key".into(),
        value: Value::Int(42),
    }).unwrap();

    // Compare
    assert!(matches!(exec, Output::Version(_)));
}
```

### 13.3 Go/No-Go Gates

M13 testing is complete when:

1. **Parity** - All 106 parity tests pass
2. **Lossless Serialization** - All types survive JSON round-trip
3. **Determinism** - Replay produces same state

### 13.4 Test Counts

| Category | Tests |
|----------|-------|
| Parity | ~106 |
| Command serialization | ~20 |
| Output serialization | ~15 |
| Error serialization | ~10 |
| Special values | ~15 |
| execute_many | ~10 |
| **M13 Total** | **~176** |

---

## 14. Known Limitations

### 14.1 No Async Execution

Commands are synchronous. Async support is deferred.

### 14.2 No Transaction Batching

`execute_atomic` is a placeholder. Transaction batching is deferred.

### 14.3 Closure Methods Excluded

`state_transition`, `state_transition_or_init`, `state_get_or_init` have no command equivalents. Clients use composition patterns.

### 14.4 No Command Middleware

Logging, metrics, tracing middleware is deferred.

### 14.5 No Remote Execution

Commands are in-process only. Wire protocol is M14+.

---

## 15. Future Extension Points

### 15.1 For M14+ (Wire Protocol)

- JSON-RPC or custom protocol
- HTTP/WebSocket transport
- Authentication/authorization layer
- Rate limiting

### 15.2 For Post-MVP (Features)

- Async execution
- Transaction batching (`execute_atomic`)
- Command middleware (logging, metrics, tracing)
- Streaming results for large result sets
- Command versioning tags

### 15.3 Extension Rules

- New commands may be added
- Existing commands are immutable
- Output types may be extended (new variants only)
- Error types may be extended (new variants only)

---

## 16. Success Criteria Checklist

### Gate 1: Command Types

- [ ] 101 command variants defined
- [ ] All commands derive required traits (Debug, Clone, Serialize, Deserialize, PartialEq)
- [ ] No Generic/Any fallbacks
- [ ] RunId type implemented

### Gate 2: Output Types

- [ ] ~40 output variants defined
- [ ] All outputs derive required traits
- [ ] VersionedValue and supporting types implemented
- [ ] Command-Output mapping documented

### Gate 3: Error Types

- [ ] ~25 error variants defined
- [ ] All errors derive required traits
- [ ] Error implements std::error::Error
- [ ] Structured details preserved

### Gate 4: Executor

- [ ] Executor struct implemented
- [ ] execute() dispatches all 101 commands
- [ ] execute_many() implemented
- [ ] Error conversion from substrate errors
- [ ] Executor is Send + Sync

### Gate 5: Serialization

- [ ] JSON encoding for all Value types
- [ ] Special value wrappers ($bytes, $f64)
- [ ] Round-trip tests passing
- [ ] No precision loss

### Gate 6: Testing

- [ ] Parity tests for all commands
- [ ] Serialization round-trip tests
- [ ] Determinism tests
- [ ] All tests passing

### Gate 7: Integration

- [ ] strata-executor crate in workspace
- [ ] Public API exports correct
- [ ] Documentation complete

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-25 | Initial M13 architecture specification |

---

**This document is the architectural specification for M13. All implementations must conform to it.**
