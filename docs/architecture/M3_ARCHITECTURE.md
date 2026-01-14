# M3 Architecture Specification: Primitives

**Version**: 1.2
**Status**: Planning Phase
**Last Updated**: 2026-01-14

---

## Executive Summary

This document specifies the architecture for **Milestone 3 (M3): Primitives** of the in-memory agent database. M3 implements all five MVP primitives as stateless facades over the transactional engine built in M1-M2.

**M3 Goals**:
- Implement KV Store primitive with full CRUD + list operations
- Implement Event Log primitive with append-only events and causal hash chaining
- Implement StateCell primitive with CAS-based coordination (renamed from StateMachine)
- Implement Trace Store primitive for structured reasoning traces
- Implement Run Index primitive for first-class run metadata management
- All primitives as stateless facades over the Database engine
- Integration tests covering primitive interactions

**Built on M1-M2**:
- M1 provides: Storage (UnifiedStore), WAL, Recovery, Run lifecycle
- M2 provides: OCC transactions, Snapshot isolation, Conflict detection, CAS operations
- M3 adds: Five high-level primitives as typed APIs over the transactional engine

**Non-Goals for M3**:
- Vector Store (M6)
- Network layer / RPC server (M7)
- MCP integration (M8)
- Advanced features: Query DSL, Run forking, Incremental snapshots (M9)

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Architecture Principles](#2-architecture-principles)
3. [Primitive Design Pattern](#3-primitive-design-pattern)
4. [KV Store Primitive](#4-kv-store-primitive)
5. [Event Log Primitive](#5-event-log-primitive)
6. [StateCell Primitive](#6-statecell-primitive)
7. [Trace Store Primitive](#7-trace-store-primitive)
8. [Run Index Primitive](#8-run-index-primitive)
9. [Key Design and Namespacing](#9-key-design-and-namespacing)
10. [Transaction Integration](#10-transaction-integration)
11. [Failure Model and Recovery](#11-failure-model-and-recovery)
12. [Invariant Enforcement](#12-invariant-enforcement)
13. [Testing Strategy](#13-testing-strategy)
14. [Performance Characteristics](#14-performance-characteristics)
15. [Migration from M2](#15-migration-from-m2)
16. [Known Limitations](#16-known-limitations)
17. [Future Extension Points](#17-future-extension-points)

---

## 1. System Overview

### 1.1 M3 Architecture Stack

```
+-------------------------------------------------------------+
|                    Application                               |
|                    (Agent Applications)                      |
+-----------------------------+-------------------------------+
                              |
                              | High-level typed APIs
                              v
+-------------------------------------------------------------+
|                   Primitives Layer (M3 NEW)                  |
|                   (Stateless Facades)                        |
|                                                              |
|  +----------+  +----------+  +------------+  +----------+   |
|  | KVStore  |  | EventLog |  | StateCell  |  |  Trace   |   |
|  |          |  |          |  |            |  |  Store   |   |
|  | - get    |  | - append |  | - read     |  | - record |   |
|  | - put    |  | - read   |  | - cas      |  | - query  |   |
|  | - delete |  | - iter   |  | - set      |  | - get    |   |
|  | - list   |  | - chain  |  |            |  |          |   |
|  +----+-----+  +----+-----+  +-----+------+  +----+-----+   |
|       |             |              |              |          |
|       +-------------+--------------+--------------+          |
|                           |                                  |
|                           |                                  |
|  +----------------------------------------------------+     |
|  |                    Run Index                        |     |
|  |                                                     |     |
|  | - create_run    - get_run     - update_status      |     |
|  | - query_runs    - end_run     - get_run_metadata   |     |
|  +------------------------+---------------------------+     |
|                           |                                  |
+---------------------------+----------------------------------+
                            |
                            | Database transaction API
                            v
+-------------------------------------------------------------+
|                    Engine Layer (M1-M2)                      |
|                                                              |
|  +-------------------------------------------------------+  |
|  |                      Database                          |  |
|  |                                                        |  |
|  | - transaction(run_id, closure)     (M2)               |  |
|  | - begin_transaction() / commit()   (M2)               |  |
|  | - put() / get() / delete() / cas() (M2 implicit)      |  |
|  +-------------------------------------------------------+  |
|                           |                                  |
+---------------------------+----------------------------------+
                            |
          +-----------------+-----------------+
          |                 |                 |
          v                 v                 v
+-----------------+ +-----------------+ +------------------+
| Storage (M1)    | | Durability (M1) | | Concurrency (M2) |
|                 | |                 | |                  |
| - UnifiedStore  | | - WAL           | | - TransactionCtx |
| - BTreeMap      | | - Recovery      | | - Snapshots      |
| - Versioning    | |                 | | - Validation     |
+-----------------+ +-----------------+ +------------------+
          |                 |                 |
          +-----------------+-----------------+
                            |
                            v
+-------------------------------------------------------------+
|                    Core Types (M1)                           |
|                                                              |
| - RunId, Namespace, Key, TypeTag                            |
| - Value, VersionedValue                                      |
| - Error, Result                                              |
| - Storage trait, SnapshotView trait                          |
+-------------------------------------------------------------+
```

### 1.2 What's New in M3

| Component | M2 Behavior | M3 Behavior |
|-----------|-------------|-------------|
| **KV Store** | Via `db.put()`/`db.get()` | Typed `KVStore` primitive with run isolation |
| **Event Log** | Not available | Append-only log with causal hash chaining |
| **StateCell** | Via `db.cas()` | Named CAS cells with versioned state |
| **Trace Store** | Not available | Structured traces: tool calls, decisions, queries |
| **Run Index** | Manual run management | First-class run lifecycle and metadata |

---

## 2. Architecture Principles

### 2.1 M3-Specific Principles

1. **Logically Stateful, Operationally Stateless**
   - Primitives hold no in-memory state themselves
   - All state lives in UnifiedStore via Database
   - Primitives are thin wrappers providing typed APIs
   - Multiple primitive instances can coexist
   - **Important**: Primitives maintain semantic state (sequences, indices, metadata) stored in UnifiedStore, but hold no in-process state. This affects reasoning about idempotency, retries, reentrancy, and replay.

2. **Run Isolation (Key Prefix Isolation)**
   - Each primitive operation is scoped to a RunId
   - Data from different runs never mixes
   - Key prefixing ensures namespace isolation
   - **Semantics**: Run isolation is key prefix isolation only, not logical execution isolation. If two runs need to share state, they must use a shared namespace (tenant/app level) or communicate through an external coordination mechanism. This is intentional—runs are meant to be reproducible in isolation.

3. **Transaction Integration**
   - All primitive operations use Database transaction API
   - Multi-primitive operations can be atomic within a transaction
   - Primitives support both implicit (single-op) and explicit (multi-op) transactions

4. **Type Safety**
   - Each primitive has its own typed API
   - TypeTag in keys prevents cross-primitive access
   - Compile-time type checking where possible

5. **Composability**
   - Primitives can be combined within transactions
   - Example: Append to EventLog and update StateCell atomically
   - No circular dependencies between primitives

6. **Invariant Enforcement**
   - Primitives are semantic enforcers, not dumb facades
   - Each primitive enforces its own invariants (see Section 12)
   - Example: EventLog enforces append-only, RunIndex enforces valid status transitions

### 2.2 Primitive Layering Rules

```
Allowed:
  Primitive -> Database -> Storage
  Primitive -> Core Types

Forbidden:
  Primitive -> Another Primitive (direct)
  Database -> Primitive
  Storage -> Primitive

Cross-Primitive:
  Application -> Multiple Primitives (within transaction) -> Database
```

---

## 3. Primitive Design Pattern

All primitives follow a consistent design pattern:

### 3.1 Common Structure

```rust
/// Generic primitive structure
pub struct PrimitiveName {
    /// Reference to database (for transactions)
    db: Arc<Database>,
}

impl PrimitiveName {
    /// Create new primitive instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Operations use transactions internally
    pub fn operation(&self, run_id: RunId, /* params */) -> Result</* return */> {
        self.db.transaction(run_id, |txn| {
            // Build key with TypeTag for this primitive
            let key = Key::new_<primitive_type>(namespace, /* key_parts */);

            // Perform operation
            // ...

            Ok(result)
        })
    }
}
```

### 3.2 Key Construction Pattern

Each primitive uses a specific TypeTag for its keys:

```rust
// KV Store keys
Key::new_kv(namespace, user_key)        // TypeTag::KV

// Event Log keys
Key::new_event(namespace, sequence_num) // TypeTag::Event

// State Machine keys
Key::new_state(namespace, machine_name) // TypeTag::State

// Trace Store keys
Key::new_trace(namespace, trace_id)     // TypeTag::Trace

// Run Index keys
Key::new_run(namespace)                 // TypeTag::Run
```

### 3.3 Transaction Context Extension

Primitives can extend `TransactionContext` for richer APIs:

```rust
/// Extension trait for primitive operations within transactions
pub trait KVTransactionExt {
    fn kv_get(&mut self, ns: &Namespace, key: &str) -> Result<Option<Value>>;
    fn kv_put(&mut self, ns: &Namespace, key: &str, value: Value) -> Result<()>;
    fn kv_delete(&mut self, ns: &Namespace, key: &str) -> Result<()>;
    fn kv_list(&mut self, ns: &Namespace, prefix: &str) -> Result<Vec<(String, Value)>>;
}

impl KVTransactionExt for TransactionContext {
    fn kv_get(&mut self, ns: &Namespace, key: &str) -> Result<Option<Value>> {
        let storage_key = Key::new_kv(ns.clone(), key);
        self.get(&storage_key)
    }
    // ...
}
```

**Extension Trait Implementation Rule**: Extension traits must delegate to primitive internals, not reimplement logic.

```rust
// CORRECT: Delegate to primitive's internal function
impl KVStoreExt for TransactionContext {
    fn kv_put(&mut self, key: &str, value: Value) -> Result<()> {
        KVStore::put_internal(self, key, value)  // Call shared internal
    }
}

// WRONG: Reimplement logic (creates maintenance burden, risks divergence)
impl KVStoreExt for TransactionContext {
    fn kv_put(&mut self, key: &str, value: Value) -> Result<()> {
        let key = Key::new_kv(...);  // Don't duplicate key construction
        self.put(key, value)
    }
}
```

---

## 4. KV Store Primitive

### 4.1 Purpose

General-purpose key-value storage for agent working memory, scratchpads, tool outputs, and ephemeral data.

### 4.2 API

```rust
pub struct KVStore {
    db: Arc<Database>,
}

impl KVStore {
    /// Create new KVStore instance
    pub fn new(db: Arc<Database>) -> Self;

    // ========== Single-Operation API (Implicit Transactions) ==========

    /// Get a value by key
    pub fn get(&self, run_id: RunId, key: &str) -> Result<Option<Value>>;

    /// Put a value
    pub fn put(&self, run_id: RunId, key: &str, value: Value) -> Result<()>;

    /// Put a value with TTL
    pub fn put_with_ttl(
        &self,
        run_id: RunId,
        key: &str,
        value: Value,
        ttl: Duration,
    ) -> Result<()>;

    /// Delete a key
    pub fn delete(&self, run_id: RunId, key: &str) -> Result<()>;

    /// List keys with optional prefix filter
    pub fn list(&self, run_id: RunId, prefix: Option<&str>) -> Result<Vec<String>>;

    /// List key-value pairs with optional prefix filter
    pub fn list_with_values(
        &self,
        run_id: RunId,
        prefix: Option<&str>,
    ) -> Result<Vec<(String, Value)>>;

    // ========== Explicit Transaction Support ==========

    /// Execute multiple KV operations atomically
    pub fn transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut KVTransaction<'_>) -> Result<T>;
}

/// Transaction handle for KV operations
pub struct KVTransaction<'a> {
    txn: &'a mut TransactionContext,
    namespace: Namespace,
}

impl<'a> KVTransaction<'a> {
    pub fn get(&mut self, key: &str) -> Result<Option<Value>>;
    pub fn put(&mut self, key: &str, value: Value) -> Result<()>;
    pub fn delete(&mut self, key: &str) -> Result<()>;
    pub fn list(&mut self, prefix: Option<&str>) -> Result<Vec<String>>;
}
```

### 4.3 Key Design

```
TypeTag: KV (0x01)
Key format: <namespace_bytes>:<TypeTag::KV>:<user_key>
```

### 4.4 Usage Examples

```rust
let kv = KVStore::new(db.clone());
let run_id = RunId::new();

// Simple operations
kv.put(run_id, "config/model", Value::String("gpt-4".into()))?;
let model = kv.get(run_id, "config/model")?;
kv.delete(run_id, "config/model")?;

// List with prefix
let config_keys = kv.list(run_id, Some("config/"))?;

// Atomic multi-key update
kv.transaction(run_id, |txn| {
    let balance = txn.get("balance")?.unwrap_or(Value::I64(0));
    let new_balance = balance.as_i64()? + 100;
    txn.put("balance", Value::I64(new_balance))?;
    txn.put("last_updated", Value::I64(timestamp()))?;
    Ok(())
})?;
```

---

## 5. Event Log Primitive

### 5.1 Purpose

Immutable, append-only event stream for capturing agent actions, observations, and state changes with causal hash chaining.

**Important Design Decisions**:

1. **EventLog is intentionally single-writer-ordered per run.** All appends to a run's event log are serialized through CAS on the metadata key. This is by design—event ordering must be total within a run. Parallel append is not supported and should not be assumed by higher layers.

2. **Causally chained, not cryptographically secure.** The hash chain provides tamper-evidence within the process boundary but does not provide tamper-resistance against storage-level modifications. The chain is not externally anchored. M4+ may upgrade to SHA-256 and external anchoring if required.

### 5.2 API

```rust
pub struct EventLog {
    db: Arc<Database>,
}

/// An event in the log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Sequence number (auto-assigned, monotonic per run)
    pub sequence: u64,
    /// Event type (user-defined category)
    pub event_type: String,
    /// Event payload (arbitrary JSON-like data)
    pub payload: Value,
    /// Timestamp when event was appended
    pub timestamp: i64,
    /// Hash of previous event (for chaining)
    pub prev_hash: [u8; 32],
    /// Hash of this event
    pub hash: [u8; 32],
}

impl EventLog {
    /// Create new EventLog instance
    pub fn new(db: Arc<Database>) -> Self;

    /// Append a new event to the log
    ///
    /// Returns the assigned sequence number and event hash
    pub fn append(
        &self,
        run_id: RunId,
        event_type: &str,
        payload: Value,
    ) -> Result<(u64, [u8; 32])>;

    /// Read a single event by sequence number
    pub fn read(&self, run_id: RunId, sequence: u64) -> Result<Option<Event>>;

    /// Read a range of events
    pub fn read_range(
        &self,
        run_id: RunId,
        start: u64,
        end: u64,
    ) -> Result<Vec<Event>>;

    /// Get the latest event (head of the log)
    pub fn head(&self, run_id: RunId) -> Result<Option<Event>>;

    /// Get the current length of the log
    pub fn len(&self, run_id: RunId) -> Result<u64>;

    /// Iterate over all events in order
    pub fn iter(&self, run_id: RunId) -> Result<EventIterator>;

    /// Verify chain integrity from start to end
    pub fn verify_chain(&self, run_id: RunId) -> Result<ChainVerification>;

    /// Read events by type
    pub fn read_by_type(
        &self,
        run_id: RunId,
        event_type: &str,
    ) -> Result<Vec<Event>>;
}

/// Chain verification result
pub struct ChainVerification {
    pub is_valid: bool,
    pub length: u64,
    pub first_invalid: Option<u64>,
    pub error: Option<String>,
}
```

### 5.3 Event Chaining

Events are chained using a simple hash function (non-cryptographic in M3, structured for future upgrade to SHA-256):

```rust
fn compute_event_hash(event: &Event, prev_hash: &[u8; 32]) -> [u8; 32] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    event.sequence.hash(&mut hasher);
    event.event_type.hash(&mut hasher);
    event.payload.hash(&mut hasher);
    event.timestamp.hash(&mut hasher);
    prev_hash.hash(&mut hasher);

    // Convert u64 hash to [u8; 32] (padded for future upgrade to SHA-256)
    let h = hasher.finish();
    let mut result = [0u8; 32];
    result[0..8].copy_from_slice(&h.to_le_bytes());
    result
}
```

### 5.4 Key Design

```
TypeTag: Event (0x02)
Key format: <namespace_bytes>:<TypeTag::Event>:<sequence_number_be_bytes>

Metadata key: <namespace_bytes>:<TypeTag::Event>:__meta__
  - stores: { next_sequence: u64, head_hash: [u8; 32] }
```

### 5.5 Usage Examples

```rust
let log = EventLog::new(db.clone());
let run_id = RunId::new();

// Append events
let (seq1, hash1) = log.append(run_id, "tool_call", json!({
    "tool": "search",
    "query": "rust async patterns"
}))?;

let (seq2, hash2) = log.append(run_id, "tool_result", json!({
    "tool": "search",
    "results": ["Pattern 1", "Pattern 2"]
}))?;

// Read events
let event = log.read(run_id, seq1)?;
let recent = log.read_range(run_id, 0, 10)?;

// Verify integrity
let verification = log.verify_chain(run_id)?;
assert!(verification.is_valid);

// Query by type
let tool_calls = log.read_by_type(run_id, "tool_call")?;
```

---

## 6. StateCell Primitive

### 6.1 Purpose

Named CAS cells for coordination records, workflow state, and atomic state transitions.

**Why "StateCell" not "StateMachine"**: In M3, this primitive is a versioned CAS cell—it stores a value with a version and supports atomic compare-and-swap updates. It does not (yet) enforce allowed transitions, guards, terminal states, or invariants. The name "StateCell" accurately reflects its current capabilities. A true "StateMachine" with transition definitions may be added in M5+.

### 6.2 API

```rust
pub struct StateCell {
    db: Arc<Database>,
}

/// State cell value with version
#[derive(Debug, Clone)]
pub struct State {
    /// Current state value
    pub value: Value,
    /// Version for CAS operations
    pub version: u64,
    /// Last updated timestamp
    pub updated_at: i64,
}

impl StateCell {
    /// Create new StateCell instance
    pub fn new(db: Arc<Database>) -> Self;

    /// Read current state
    ///
    /// Returns None if state cell doesn't exist
    pub fn read(&self, run_id: RunId, name: &str) -> Result<Option<State>>;

    /// Initialize state cell with initial value
    ///
    /// Fails if already exists (use CAS for updates)
    pub fn init(&self, run_id: RunId, name: &str, initial: Value) -> Result<()>;

    /// Compare-and-swap state
    ///
    /// Atomically updates state only if current version matches expected
    pub fn cas(
        &self,
        run_id: RunId,
        name: &str,
        expected_version: u64,
        new_value: Value,
    ) -> Result<u64>;

    /// Force-set state (unconditional write)
    ///
    /// Use with caution - bypasses version check
    pub fn set(&self, run_id: RunId, name: &str, value: Value) -> Result<u64>;

    /// Delete state cell
    pub fn delete(&self, run_id: RunId, name: &str) -> Result<()>;

    /// List all state cell names
    pub fn list(&self, run_id: RunId) -> Result<Vec<String>>;

    /// Check if state cell exists
    pub fn exists(&self, run_id: RunId, name: &str) -> Result<bool>;

    /// Execute state transition within transaction (with automatic retry on conflict)
    pub fn transition<F, T>(
        &self,
        run_id: RunId,
        name: &str,
        f: F,
    ) -> Result<T>
    where
        F: Fn(&State) -> Result<(Value, T)>;
}
```

### 6.3 Key Design

```
TypeTag: State (0x03)
Key format: <namespace_bytes>:<TypeTag::State>:<cell_name>
```

### 6.4 Purity Requirement

**Closures passed to `transition()` must be pure functions.**

The `transition()` method may execute its closure multiple times due to OCC retries. Therefore:

| Requirement | Explanation |
|-------------|-------------|
| **Pure function of inputs** | Closure result must depend only on the `&State` argument |
| **No I/O** | No file, network, or console operations inside the closure |
| **No external mutation** | Do not modify variables outside the closure scope |
| **No irreversible effects** | No logging, metrics, or external API calls |
| **Idempotent** | Multiple executions with same input produce same result |

**Example - CORRECT:**
```rust
sc.transition(run_id, "counter", |state| {
    let current = state.value.as_i64().unwrap_or(0);
    let new_value = Value::I64(current + 1);
    Ok((new_value, current + 1))  // Pure computation
})?;
```

**Example - WRONG:**
```rust
sc.transition(run_id, "counter", |state| {
    println!("Incrementing counter");  // WRONG: I/O
    external_counter.fetch_add(1);     // WRONG: External mutation
    let current = state.value.as_i64().unwrap_or(0);
    Ok((Value::I64(current + 1), current + 1))
})?;
```

**Why this matters**: If a transaction retries 3 times, the closure executes 3 times. Side effects would be tripled. Keep closures pure; perform side effects after `transition()` returns successfully.

### 6.5 Usage Examples

```rust
let sc = StateCell::new(db.clone());
let run_id = RunId::new();

// Initialize workflow state
sc.init(run_id, "workflow/status", Value::String("pending".into()))?;

// Read current state
let state = sc.read(run_id, "workflow/status")?.unwrap();
assert_eq!(state.value, Value::String("pending".into()));

// CAS update
sc.cas(
    run_id,
    "workflow/status",
    state.version,
    Value::String("running".into()),
)?;

// Transition with closure (retry-safe, closure may be called multiple times)
let result = sc.transition(run_id, "counter", |state| {
    let current = state.value.as_i64().unwrap_or(0);
    let new_value = Value::I64(current + 1);
    Ok((new_value, current + 1))
})?;
```

---

## 7. Trace Store Primitive

### 7.1 Purpose

Structured storage for agent reasoning traces including tool calls, decisions, queries, and thought processes.

**Performance Warning**: TraceStore is optimized for debuggability, not ingestion throughput. The 3-4 secondary index entries per trace create write amplification. For high-volume tracing, consider batching or sampling. This primitive is designed for reasoning traces (tens to hundreds per run), not telemetry (thousands per second).

### 7.2 API

```rust
pub struct TraceStore {
    db: Arc<Database>,
}

/// Trace entry type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TraceType {
    /// Tool invocation
    ToolCall {
        tool_name: String,
        arguments: Value,
        result: Option<Value>,
        duration_ms: Option<u64>,
    },
    /// Decision point
    Decision {
        question: String,
        options: Vec<String>,
        chosen: String,
        reasoning: Option<String>,
    },
    /// Query/search
    Query {
        query_type: String,
        query: String,
        results_count: usize,
    },
    /// Thought/reasoning step
    Thought {
        content: String,
        confidence: Option<f64>,
    },
    /// Error/exception
    Error {
        error_type: String,
        message: String,
        recoverable: bool,
    },
    /// Custom trace type
    Custom {
        trace_type: String,
        data: Value,
    },
}

/// A trace entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    /// Unique trace ID
    pub id: String,
    /// Parent trace ID (for nested traces)
    pub parent_id: Option<String>,
    /// Trace type and data
    pub trace_type: TraceType,
    /// Timestamp
    pub timestamp: i64,
    /// Optional tags for filtering
    pub tags: Vec<String>,
    /// Optional metadata
    pub metadata: Option<Value>,
}

impl TraceStore {
    /// Create new TraceStore instance
    pub fn new(db: Arc<Database>) -> Self;

    /// Record a new trace
    ///
    /// Returns the assigned trace ID
    pub fn record(&self, run_id: RunId, trace_type: TraceType) -> Result<String>;

    /// Record a trace with parent (for nesting)
    pub fn record_child(
        &self,
        run_id: RunId,
        parent_id: &str,
        trace_type: TraceType,
    ) -> Result<String>;

    /// Record a trace with custom ID and tags
    pub fn record_with_options(
        &self,
        run_id: RunId,
        trace_type: TraceType,
        options: TraceOptions,
    ) -> Result<String>;

    /// Get a trace by ID
    pub fn get(&self, run_id: RunId, trace_id: &str) -> Result<Option<Trace>>;

    /// Query traces by type
    pub fn query_by_type(
        &self,
        run_id: RunId,
        trace_type_name: &str,
    ) -> Result<Vec<Trace>>;

    /// Query traces by tag
    pub fn query_by_tag(&self, run_id: RunId, tag: &str) -> Result<Vec<Trace>>;

    /// Query traces in time range
    pub fn query_by_time(
        &self,
        run_id: RunId,
        start: i64,
        end: i64,
    ) -> Result<Vec<Trace>>;

    /// Get all child traces of a parent
    pub fn get_children(&self, run_id: RunId, parent_id: &str) -> Result<Vec<Trace>>;

    /// Get trace tree (recursive)
    pub fn get_tree(&self, run_id: RunId, root_id: &str) -> Result<TraceTree>;

    /// List all trace IDs
    pub fn list(&self, run_id: RunId) -> Result<Vec<String>>;

    /// Count traces
    pub fn count(&self, run_id: RunId) -> Result<usize>;
}

/// Options for recording traces
#[derive(Default)]
pub struct TraceOptions {
    pub id: Option<String>,
    pub parent_id: Option<String>,
    pub tags: Vec<String>,
    pub metadata: Option<Value>,
}

/// Hierarchical trace tree
pub struct TraceTree {
    pub root: Trace,
    pub children: Vec<TraceTree>,
}
```

### 7.3 Key Design

```
TypeTag: Trace (0x04)
Key format: <namespace_bytes>:<TypeTag::Trace>:<trace_id>

Index keys for queries:
  By type:   <namespace_bytes>:<TypeTag::Trace>:__idx_type__:<type>:<trace_id>
  By tag:    <namespace_bytes>:<TypeTag::Trace>:__idx_tag__:<tag>:<trace_id>
  By parent: <namespace_bytes>:<TypeTag::Trace>:__idx_parent__:<parent_id>:<trace_id>
  By time:   <namespace_bytes>:<TypeTag::Trace>:__idx_time__:<timestamp_be>:<trace_id>
```

### 7.4 Usage Examples

```rust
let traces = TraceStore::new(db.clone());
let run_id = RunId::new();

// Record tool call
let tool_trace = traces.record(run_id, TraceType::ToolCall {
    tool_name: "web_search".into(),
    arguments: json!({"query": "rust async"}),
    result: Some(json!(["result1", "result2"])),
    duration_ms: Some(150),
})?;

// Record decision with reasoning
traces.record(run_id, TraceType::Decision {
    question: "Which search result to use?".into(),
    options: vec!["result1".into(), "result2".into()],
    chosen: "result1".into(),
    reasoning: Some("More relevant to the query".into()),
})?;

// Record nested trace (child of tool call)
traces.record_child(run_id, &tool_trace, TraceType::Thought {
    content: "Analyzing search results...".into(),
    confidence: Some(0.85),
})?;

// Query traces
let tool_calls = traces.query_by_type(run_id, "ToolCall")?;
let tree = traces.get_tree(run_id, &tool_trace)?;
```

---

## 8. Run Index Primitive

### 8.1 Purpose

First-class run lifecycle management with metadata, status tracking, and run relationships.

### 8.2 API

```rust
pub struct RunIndex {
    db: Arc<Database>,
}

/// Run status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    /// Run is active and accepting operations
    Active,
    /// Run completed successfully
    Completed,
    /// Run failed with error
    Failed,
    /// Run was cancelled
    Cancelled,
    /// Run is paused
    Paused,
    /// Run is archived (soft-deleted, excluded from default queries)
    Archived,
}

/// Run metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    /// Run identifier
    pub run_id: RunId,
    /// Parent run (if forked)
    pub parent_run: Option<RunId>,
    /// Run status
    pub status: RunStatus,
    /// Creation timestamp
    pub created_at: i64,
    /// Last updated timestamp
    pub updated_at: i64,
    /// Completion timestamp (if finished)
    pub completed_at: Option<i64>,
    /// User-defined tags
    pub tags: Vec<String>,
    /// User-defined metadata
    pub metadata: Value,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Query filter for runs
pub struct RunQuery {
    pub status: Option<RunStatus>,
    pub tags: Option<Vec<String>>,
    pub created_after: Option<i64>,
    pub created_before: Option<i64>,
    pub parent_run: Option<RunId>,
    pub limit: Option<usize>,
}

impl RunIndex {
    /// Create new RunIndex instance
    pub fn new(db: Arc<Database>) -> Self;

    /// Create a new run
    pub fn create_run(&self, namespace: &Namespace) -> Result<RunMetadata>;

    /// Create a new run with options
    pub fn create_run_with_options(
        &self,
        namespace: &Namespace,
        options: CreateRunOptions,
    ) -> Result<RunMetadata>;

    /// Get run metadata
    pub fn get_run(&self, run_id: RunId) -> Result<Option<RunMetadata>>;

    /// Update run status
    pub fn update_status(
        &self,
        run_id: RunId,
        status: RunStatus,
    ) -> Result<()>;

    /// Update run status with error message
    pub fn fail_run(&self, run_id: RunId, error: &str) -> Result<()>;

    /// Complete run successfully
    pub fn complete_run(&self, run_id: RunId) -> Result<()>;

    /// Add tags to run
    pub fn add_tags(&self, run_id: RunId, tags: &[String]) -> Result<()>;

    /// Update run metadata
    pub fn update_metadata(
        &self,
        run_id: RunId,
        metadata: Value,
    ) -> Result<()>;

    /// Query runs with filters
    pub fn query_runs(&self, query: RunQuery) -> Result<Vec<RunMetadata>>;

    /// List all run IDs
    pub fn list_runs(&self) -> Result<Vec<RunId>>;

    /// Get child runs (forked from parent)
    pub fn get_child_runs(&self, parent_run: RunId) -> Result<Vec<RunMetadata>>;

    /// Delete run and all associated data (CASCADING HARD DELETE)
    ///
    /// This performs a cascading hard delete:
    /// 1. All keys with the run's namespace prefix are deleted (KV, Events, States, Traces)
    /// 2. Run metadata is deleted
    /// 3. All secondary indices referencing this run are deleted
    ///
    /// This is IRREVERSIBLE. Use archive_run() for soft delete.
    pub fn delete_run(&self, run_id: RunId) -> Result<()>;

    /// Archive run (soft delete)
    ///
    /// Sets status to Archived without deleting data.
    /// Archived runs are excluded from queries by default but data remains accessible.
    pub fn archive_run(&self, run_id: RunId) -> Result<()>;

    /// Get run statistics
    pub fn get_stats(&self) -> Result<RunStats>;
}

/// Options for creating a run
#[derive(Default)]
pub struct CreateRunOptions {
    pub run_id: Option<RunId>,
    pub parent_run: Option<RunId>,
    pub tags: Vec<String>,
    pub metadata: Value,
}

/// Run statistics
pub struct RunStats {
    pub total_runs: usize,
    pub active_runs: usize,
    pub completed_runs: usize,
    pub failed_runs: usize,
}
```

### 8.3 Key Design

```
TypeTag: Run (0x05)
Key format (run data):  <namespace_bytes>:<TypeTag::Run>:<run_id_bytes>
Key format (index):     <namespace_bytes>:<TypeTag::Run>:__idx_status__:<status>:<run_id>
Key format (by tag):    <namespace_bytes>:<TypeTag::Run>:__idx_tag__:<tag>:<run_id>
Key format (by parent): <namespace_bytes>:<TypeTag::Run>:__idx_parent__:<parent_id>:<run_id>
```

### 8.4 Usage Examples

```rust
let runs = RunIndex::new(db.clone());
let ns = Namespace::new("tenant", "app", "agent", RunId::new());

// Create new run
let run_meta = runs.create_run(&ns)?;
let run_id = run_meta.run_id;

// Create run with options
let child_run = runs.create_run_with_options(&ns, CreateRunOptions {
    parent_run: Some(run_id),
    tags: vec!["retry".into(), "experiment".into()],
    ..Default::default()
})?;

// Update status
runs.update_status(run_id, RunStatus::Completed)?;

// Query runs
let active = runs.query_runs(RunQuery {
    status: Some(RunStatus::Active),
    ..Default::default()
})?;

let by_tag = runs.query_runs(RunQuery {
    tags: Some(vec!["experiment".into()]),
    ..Default::default()
})?;

// Get children
let children = runs.get_child_runs(run_id)?;
```

---

## 9. Key Design and Namespacing

### 9.1 TypeTag Values

```rust
#[repr(u8)]
pub enum TypeTag {
    KV = 0x01,
    Event = 0x02,
    State = 0x03,
    Trace = 0x04,
    Run = 0x05,
    // Reserved for M6+
    Vector = 0x10,
}
```

### 9.2 Key Structure

All keys follow the same structure:

```
[namespace_bytes][separator][type_tag][separator][primitive_specific_key]
```

Where:
- `namespace_bytes`: Serialized namespace (tenant/app/agent/run)
- `separator`: `0x00` byte
- `type_tag`: Single byte from TypeTag enum
- `primitive_specific_key`: Varies by primitive

### 9.3 Key Ordering

Keys are ordered lexicographically, which enables:
- Efficient prefix scans per namespace
- Efficient scans per primitive type within namespace
- Natural ordering for sequence numbers (using big-endian encoding)

### 9.4 Example Key Bytes

```
KV key "user/name" for run abc123:
  [tenant:app:agent:abc123][0x00][0x01][user/name]

Event key for sequence 42:
  [tenant:app:agent:abc123][0x00][0x02][0x00 0x00 0x00 0x00 0x00 0x00 0x00 0x2A]

State key "workflow/status":
  [tenant:app:agent:abc123][0x00][0x03][workflow/status]
```

### 9.5 Namespace Scope

**Run-scoped vs Shared Namespaces**

The hierarchical namespace `tenant/app/agent/run` supports two scoping patterns:

| Scope | Namespace Pattern | Use Case |
|-------|-------------------|----------|
| **Run-scoped** | `tenant/app/agent/run_id` | Per-run isolated data (default, recommended) |
| **Agent-shared** | `tenant/app/agent/_shared` | Configuration shared across runs of same agent |
| **App-shared** | `tenant/app/_shared/_shared` | Data shared across agents in same app |
| **Tenant-shared** | `tenant/_shared/_shared/_shared` | Tenant-wide configuration |

**Run-scoped (Default)**:
- All primitive APIs take `run_id` and construct the full namespace
- Data is automatically isolated per run
- No coordination needed between runs

```rust
// Run-scoped: each run has its own "counter" key
kv.put(run_id_1, "counter", Value::I64(1))?;  // run_id_1 has counter=1
kv.put(run_id_2, "counter", Value::I64(2))?;  // run_id_2 has counter=2
// No conflict - keys are in different namespaces
```

**Shared Namespaces (Advanced)**:
- Application explicitly constructs a shared namespace
- Multiple runs can read/write same keys
- **Requires explicit coordination** (CAS, transactions) to avoid conflicts
- Use with caution—violates run isolation principle

```rust
// Shared: explicit namespace construction (bypasses run_id scoping)
let shared_ns = Namespace::agent_shared("tenant", "app", "agent");
kv.put_with_namespace(&shared_ns, "global_config", config)?;
// All runs see the same key - coordination needed!
```

**Rule of Thumb**:
- Default to run-scoped for all primitive data
- Use shared namespaces only for truly global configuration (feature flags, model settings)
- Never use shared namespaces for mutable per-request state

---

## 10. Transaction Integration

### 10.1 Cross-Primitive Transactions

Primitives can be combined within a single transaction:

```rust
// Atomic: append event + update state cell + record trace
db.transaction(run_id, |txn| {
    // Use extension traits on TransactionContext
    txn.event_append("task_completed", json!({"task_id": 123}))?;
    txn.state_cas("task/123/status", current_version, Value::String("done".into()))?;
    txn.trace_record(TraceType::Thought {
        content: "Task completed successfully".into(),
        confidence: Some(1.0),
    })?;
    Ok(())
})?;
```

### 10.2 Extension Traits

Each primitive provides an extension trait for `TransactionContext`:

```rust
pub trait EventLogExt {
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64>;
    fn event_read(&mut self, sequence: u64) -> Result<Option<Event>>;
}

pub trait StateCellExt {
    fn state_read(&mut self, name: &str) -> Result<Option<State>>;
    fn state_cas(&mut self, name: &str, expected: u64, value: Value) -> Result<u64>;
}

pub trait TraceStoreExt {
    fn trace_record(&mut self, trace_type: TraceType) -> Result<String>;
}

pub trait KVStoreExt {
    fn kv_get(&mut self, key: &str) -> Result<Option<Value>>;
    fn kv_put(&mut self, key: &str, value: Value) -> Result<()>;
}
```

### 10.3 Implicit vs Explicit Transactions

| Use Case | API | Transaction Type |
|----------|-----|------------------|
| Single primitive operation | `kv.put(run_id, key, value)` | Implicit |
| Multiple same-primitive ops | `kv.transaction(run_id, \|txn\| { ... })` | Explicit |
| Cross-primitive atomic ops | `db.transaction(run_id, \|txn\| { ... })` | Explicit |

---

## 11. Failure Model and Recovery

### 11.1 Transaction Failure Model

**All-or-nothing with no sequence number reuse.**

When a transaction fails:

1. **Partial writes are never applied** - The write_set is discarded entirely
2. **Indices are never partially created** - All index writes are in the same transaction
3. **EventLog sequence numbers are contiguous** - Failed transactions don't "burn" sequences because metadata updates are transactional

```
Sequence timeline (correct behavior):
  0 -> 1 -> 2 -> [FAILED: transaction rolled back] -> 3 -> 4
                  ^ metadata stays at 3 (rollback)
                  ^ retry gets sequence 3 again

Result: sequences are contiguous (0, 1, 2, 3, 4, ...)
```

**Why sequences are contiguous**: The metadata CAS (next_sequence) and event write are in the same transaction. If the transaction fails, both the event write AND the metadata update are rolled back. The next attempt reads the same sequence number.

### 11.2 Recovery Contract

**WAL replay reconstructs all state including indices.**

After crash + WAL replay:

| What | Behavior |
|------|----------|
| **Sequence numbers** | Preserved (stored in event metadata key, replayed from WAL) |
| **Secondary indices** | Replayed, not rebuilt (index writes are in WAL as regular writes) |
| **Derived keys (hashes)** | Stored, not recomputed (hashes are stored with events) |

**Implications**:
- Recovery is O(WAL size), not O(data size × index factor)
- If indices become corrupted, there's no automatic rebuild (would need explicit repair command)
- Hash chains are verified on read, not on recovery

**Future consideration (M4)**: Add optional `--verify-on-recovery` flag that validates hash chains during replay.

### 11.3 Replay Source Hierarchy

**WAL is the canonical source. EventLog is a semantic overlay.**

| Replay Type | Source | Purpose |
|-------------|--------|---------|
| **Crash recovery** | WAL | Reconstruct database state (all primitives) |
| **Agent replay (M5)** | EventLog | Verify agent decisions are deterministic |

```
WAL replay: Reconstruct database state
  WAL -> Storage -> All primitives restored

EventLog replay: Reconstruct agent execution (M5)
  EventLog -> Agent logic -> Verify same decisions
```

**Important**: EventLog replay is for **verification**, not **recovery**. You cannot recover database state from EventLog alone—you need the WAL.

### 11.4 Index Consistency Contract

**What happens when indices are corrupt or missing?**

Secondary indices (TraceStore by-type, by-tag, by-parent; RunIndex by-status, by-tag) can become inconsistent if:
1. Storage corruption (unlikely but possible)
2. Bug in primitive implementation
3. Manual storage manipulation (e.g., direct key deletion)

| Scenario | Behavior | Recovery |
|----------|----------|----------|
| **Index points to missing data** | Query returns incomplete results | Run `repair_indices()` (future M4) |
| **Data exists without index** | Data unreachable via query, but direct access works | Run `rebuild_indices()` (future M4) |
| **Index has wrong value** | Query returns wrong results | Run `verify_indices()` then `rebuild_indices()` |

**M3 Behavior (No Automatic Repair)**:
- Queries return whatever the index contains (no verification on read)
- No automatic detection or repair
- Manual inspection via `list()` can reveal orphaned data

**Future M4 Tools**:
```rust
// Verify index consistency
db.verify_indices(run_id) -> IndexVerification

// Rebuild all indices from primary data
db.rebuild_indices(run_id) -> Result<()>

// Repair specific index
db.repair_index(run_id, IndexType::TraceByTag) -> Result<()>
```

**Prevention**:
- All index writes are in the same transaction as primary data writes
- Normal operation should never create inconsistencies
- WAL replay preserves index consistency (indices are replayed, not rebuilt)

---

## 12. Invariant Enforcement

Primitives are **semantic enforcers**, not dumb facades. Each primitive enforces its own invariants.

### 12.1 Invariant Table

| Primitive | Enforced Invariants |
|-----------|---------------------|
| **KVStore** | Key uniqueness per namespace (put overwrites, no duplicates) |
| **EventLog** | Append-only (no update/delete), monotonic sequences, chain integrity |
| **StateCell** | Version monotonicity (CAS cannot go backward on same key) |
| **TraceStore** | ID uniqueness, parent must exist for child traces |
| **RunIndex** | Status transition validity (see below) |

### 12.2 RunIndex Status Transitions

```rust
fn is_valid_transition(from: RunStatus, to: RunStatus) -> bool {
    match (from, to) {
        // From Active
        (Active, Completed) => true,
        (Active, Failed) => true,
        (Active, Cancelled) => true,
        (Active, Paused) => true,
        (Active, Archived) => true,

        // From Paused
        (Paused, Active) => true,
        (Paused, Cancelled) => true,
        (Paused, Archived) => true,

        // From terminal states (Completed/Failed/Cancelled)
        (Completed, Archived) => true,
        (Failed, Archived) => true,
        (Cancelled, Archived) => true,

        // Everything else is invalid
        _ => false,  // No resurrection, no re-failing, no un-archiving
    }
}
```

**Key rules**:
- No resurrection: Cannot go from Completed/Failed/Cancelled back to Active
- No re-failing: Cannot fail an already-failed run
- Archived is terminal: Cannot un-archive

### 12.3 EventLog Invariant Enforcement

```rust
impl EventLog {
    /// Update is NOT allowed - EventLog is append-only
    pub fn update(&self, ...) -> Result<()> {
        Err(Error::InvalidOperation("EventLog is append-only. Use append() instead."))
    }

    /// Delete is NOT allowed - EventLog is immutable
    pub fn delete(&self, ...) -> Result<()> {
        Err(Error::InvalidOperation("EventLog is immutable. Events cannot be deleted."))
    }
}
```

### 12.4 No Direct Storage Mutation

**All application data mutations must go through primitives.**

Bypassing primitives by calling `db.put()` / `db.delete()` directly breaks invariants:

| Violation | Consequence |
|-----------|-------------|
| Direct write to Event key | Hash chain broken, sequence gaps possible |
| Direct delete of State key | Version history lost, CAS may behave unexpectedly |
| Direct write to index key | Index inconsistency (orphan pointers) |
| Direct write to Run metadata | Status transitions may violate state machine |

**Allowed Direct Access**:
| Operation | Allowed? | Reason |
|-----------|----------|--------|
| `db.get()` for debugging | ✅ Yes | Read-only, no mutation |
| `db.put()` in migration script | ⚠️ Careful | Must maintain invariants manually |
| `db.delete()` for cleanup | ❌ No | Use `RunIndex.delete_run()` for cascading delete |

**Rule**: If you need to mutate data, use the primitive API. If the primitive doesn't expose the operation you need, that's a signal that the operation may violate invariants.

```rust
// WRONG: Direct storage mutation
db.put(run_id, Key::new_event(ns, 5), event_value)?;  // Breaks chain!

// CORRECT: Use primitive
event_log.append(run_id, "event_type", payload)?;  // Maintains chain
```

**Exception**: Internal primitive implementations necessarily call `db.put()` / `db.get()`. The rule applies to application code, not primitive internals.

---

## 13. Testing Strategy

### 13.1 Unit Tests (Per Primitive)

Each primitive has its own test module:

```rust
#[cfg(test)]
mod tests {
    #[test] fn test_kv_get_put() { ... }
    #[test] fn test_kv_list_with_prefix() { ... }
    #[test] fn test_kv_transaction_atomicity() { ... }
}
```

### 13.2 Integration Tests

Cross-primitive scenarios:

```rust
#[test]
fn test_event_log_plus_state_cell() {
    // Atomic: append event + update state
}

#[test]
fn test_trace_with_kv_operations() {
    // Record traces for KV operations
}

#[test]
fn test_run_lifecycle_with_all_primitives() {
    // Create run, use all primitives, complete run
}
```

### 13.3 Isolation Tests

Verify run isolation:

```rust
#[test]
fn test_run_isolation() {
    let run1 = RunId::new();
    let run2 = RunId::new();

    kv.put(run1, "key", Value::I64(1))?;
    kv.put(run2, "key", Value::I64(2))?;

    // Each run sees only its own data
    assert_eq!(kv.get(run1, "key")?, Some(Value::I64(1)));
    assert_eq!(kv.get(run2, "key")?, Some(Value::I64(2)));
}
```

### 13.4 Recovery Tests

Verify primitives work after recovery:

```rust
#[test]
fn test_recovery_preserves_events() {
    // Write events
    log.append(run_id, "event1", payload)?;
    log.append(run_id, "event2", payload)?;

    // Close and reopen database
    drop(db);
    let db = Database::open(path)?;
    let log = EventLog::new(db.clone());

    // Events should survive
    assert_eq!(log.len(run_id)?, 2);
    assert!(log.verify_chain(run_id)?.is_valid);
}
```

---

## 14. Performance Characteristics

### 14.1 Expected Performance

| Operation | Complexity | Expected Latency |
|-----------|------------|------------------|
| KV get/put/delete | O(log n) | < 1ms |
| KV list | O(k log n) | k × < 1ms |
| Event append | O(log n) | < 1ms |
| Event read | O(log n) | < 1ms |
| Event read_range | O(k log n) | k × < 1ms |
| State read/cas | O(log n) | < 1ms |
| Trace record | O(log n) | < 1ms |
| Trace query (by index) | O(k log n) | k × < 1ms |
| Run create/update | O(log n) | < 1ms |
| Run query | O(k log n) | k × < 1ms |

### 14.2 Index Overhead

Trace Store and Run Index maintain secondary indices:

| Primitive | Index Overhead |
|-----------|----------------|
| KV Store | None |
| Event Log | 1 metadata key per run |
| StateCell | None |
| Trace Store | 3-4 index entries per trace |
| Run Index | 2-3 index entries per run |

### 14.3 Memory Usage

All primitives are stateless facades. Memory usage is dominated by:
- Database storage (BTreeMap)
- Active transaction snapshots
- No additional memory per primitive instance

---

## 15. Migration from M2

### 15.1 Backwards Compatibility

M2 code continues to work:

```rust
// M2 code (still works)
db.put(run_id, key, value)?;
db.get(&key)?;
db.transaction(run_id, |txn| { ... })?;

// M3 code (new primitives)
let kv = KVStore::new(db.clone());
kv.put(run_id, "key", value)?;
```

### 15.2 Migration Path

1. **Phase 1**: Add primitives alongside existing code
   - M2 `db.put()`/`db.get()` still work
   - New code uses `KVStore`, `EventLog`, etc.

2. **Phase 2**: Migrate existing code to primitives
   - Replace `db.put()/get()` with `KVStore` methods
   - Use typed APIs for better safety

3. **Phase 3**: Use cross-primitive transactions
   - Combine operations atomically
   - Use extension traits

---

## 16. Known Limitations

### 16.1 M3 Limitations

| Limitation | Impact | Mitigation Plan |
|------------|--------|-----------------|
| **Non-crypto hash in EventLog** | Chain integrity is not cryptographically secure | M4+: Upgrade to SHA-256 |
| **Linear trace queries** | Query by type/tag scans all matching keys | M4+: Add B-tree indices |
| **No vector search** | Cannot do semantic similarity | M6: Vector Store primitive |
| **No run forking** | Cannot branch runs | M9: Run forking feature |

### 16.2 What M3 Does NOT Provide

- Vector Store (M6)
- Network layer / RPC (M7)
- MCP integration (M8)
- Query DSL (M9)
- Run forking (M9)
- Distributed mode (far future)

---

## 17. Future Extension Points

### 17.1 M4: Durability Enhancements

- Snapshot + WAL rotation
- Point-in-time recovery
- Configurable retention

### 17.2 M5: Replay

- `replay_run(run_id)` reconstructs state
- Event Log enables deterministic replay
- Run Index provides O(run size) replay

### 17.3 M5+: Real StateMachine (upgrade from StateCell)

```rust
pub struct StateMachine {
    db: Arc<Database>,
}

impl StateMachine {
    /// Define allowed state transitions
    pub fn define_transitions(&self, run_id: RunId, name: &str, transitions: Vec<Transition>) -> Result<()>;

    /// Transition with guard validation
    pub fn transition(&self, run_id: RunId, name: &str, event: &str) -> Result<State>;

    /// Check if in terminal state
    pub fn is_terminal(&self, run_id: RunId, name: &str) -> Result<bool>;
}

pub struct Transition {
    pub from: String,
    pub to: String,
    pub event: String,
    pub guard: Option<Box<dyn Fn(&State) -> bool>>,
}
```

### 17.4 M6: Vector Store

```rust
pub struct VectorStore {
    db: Arc<Database>,
}

impl VectorStore {
    pub fn insert(&self, run_id: RunId, id: &str, vector: &[f32], metadata: Value) -> Result<()>;
    pub fn search(&self, run_id: RunId, query: &[f32], k: usize) -> Result<Vec<SearchResult>>;
    pub fn delete(&self, run_id: RunId, id: &str) -> Result<()>;
}
```

### 17.5 M9: Advanced Features

- Query DSL for complex filters
- Run forking and lineage tracking
- Incremental snapshots

---

## 18. Appendix

### 18.1 Crate Structure (Updated)

```
in-mem/
+-- crates/
    +-- core/                     # M1 (unchanged)
    +-- storage/                  # M1 (unchanged)
    +-- durability/               # M1 (unchanged)
    +-- concurrency/              # M2 (unchanged)
    +-- engine/                   # M1-M2 (unchanged)
    +-- primitives/               # M3 (NEW - major additions)
        +-- src/
            +-- lib.rs            # Re-exports all primitives
            +-- kv.rs             # KVStore primitive
            +-- event_log.rs      # EventLog primitive
            +-- state_cell.rs     # StateCell primitive
            +-- trace.rs          # TraceStore primitive
            +-- run_index.rs      # RunIndex primitive
            +-- extensions.rs     # Transaction extension traits
    +-- api/                      # High-level API (future)
```

### 18.2 Dependencies (Updated)

**New Dependencies for primitives crate**:
- `serde` / `serde_json`: Serialization for Event, Trace, RunMetadata
- `uuid`: ID generation (already in core)

**Internal Dependencies**:
```
primitives -> engine, core
engine -> storage, durability, concurrency, core
```

---

## Conclusion

M3 adds **five high-level primitives** to the transactional foundation built in M1-M2:

- **KV Store**: General-purpose key-value with run isolation
- **Event Log**: Immutable append-only events with causal hash chaining
- **StateCell**: CAS-based versioned cells (not yet a full state machine)
- **Trace Store**: Structured reasoning traces with indexing (optimized for debuggability)
- **Run Index**: First-class run lifecycle management with status transition validation

**Key Design Decisions**:
- Primitives are logically stateful but operationally stateless
- Run isolation is key prefix isolation (not logical execution isolation)
- EventLog is single-writer-ordered per run
- Hash chaining is causal, not cryptographically secure
- Primitives enforce invariants (not dumb facades)
- All-or-nothing transaction failure model with contiguous sequences
- Transition closures must be pure (may execute multiple times)
- All mutations must go through primitives (no direct storage access)

**Success Criteria**:
- [ ] KV store: get, put, delete, list working
- [ ] Event log: append, read, chain verification working
- [ ] StateCell: read, CAS, transitions working
- [ ] Trace store: record, query by type/tag/time working
- [ ] Run Index: create, update, query, lifecycle working
- [ ] All primitives enforce their invariants
- [ ] Integration tests cover primitive interactions
- [ ] Run isolation verified
- [ ] Recovery preserves all primitive data
- [ ] Status transition validation enforced

**Next**: M4 adds periodic snapshots, WAL rotation, and configurable durability.

---

**Document Version**: 1.2
**Status**: Planning Phase (Ship-Ready)
**Date**: 2026-01-14
