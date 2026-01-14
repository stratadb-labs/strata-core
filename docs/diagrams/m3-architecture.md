# M3 Architecture Diagrams: Primitives

This document contains visual representations of the M3 architecture with all five MVP primitives.

---

## 1. System Architecture Overview (M3)

```
+-------------------------------------------------------------------------+
|                           Application Layer                              |
|                      (Agent Applications using DB)                       |
+-----------------------------------+-------------------------------------+
                                    |
                                    | High-level typed APIs
                                    v
+-------------------------------------------------------------------------+
|                          Primitives Layer (M3 NEW)                       |
|                          (Stateless Facades)                             |
|                                                                          |
|  +-------------+  +-------------+  +--------------+  +-------------+    |
|  |  KV Store   |  |  Event Log  |  |  StateCell   |  |Trace Store  |    |
|  |             |  |             |  |              |  |             |    |
|  | - get()     |  | - append()  |  | - read()     |  | - record()  |    |
|  | - put()     |  | - read()    |  | - init()     |  | - get()     |    |
|  | - delete()  |  | - iter()    |  | - cas()      |  | - query_*() |    |
|  | - list()    |  | - verify()  |  | - set()      |  | - get_tree()|    |
|  +------+------+  +------+------+  +------+-------+  +------+------+    |
|         |                |                |                |            |
|         +----------------+----------------+----------------+            |
|                                   |                                     |
|                                   |                                     |
|  +-----------------------------------------------------------------+   |
|  |                         Run Index                                 |   |
|  |                                                                   |   |
|  | - create_run()    - get_run()       - update_status()            |   |
|  | - query_runs()    - complete_run()  - fail_run()                 |   |
|  | - list_runs()     - delete_run()    - get_child_runs()           |   |
|  +----------------------------+--------------------------------------+   |
|                               |                                         |
+-------------------------------+-----------------------------------------+
                                |
                                | Database transaction API
                                v
+-------------------------------------------------------------------------+
|                         Engine Layer (M1-M2)                             |
|                   (Orchestration & Coordination)                         |
|                                                                          |
|  +-------------------------------------------------------------------+  |
|  |                          Database                                  |  |
|  |                                                                    |  |
|  |  Transaction API (M2):                                            |  |
|  |  - transaction(run_id, closure)                                   |  |
|  |  - begin_transaction() / commit_transaction()                     |  |
|  |  - put() / get() / delete() / cas() [implicit transactions]       |  |
|  |                                                                    |  |
|  |  Run Lifecycle (M1):                                              |  |
|  |  - begin_run() / end_run()                                        |  |
|  |  - recovery on startup                                            |  |
|  +-------------------------------------------------------------------+  |
|                               |                                          |
+---------+--------------------+--------------------+---------------------+
          |                    |                    |
          v                    v                    v
+------------------+  +-------------------+  +---------------------+
|  Storage (M1)    |  | Durability (M1)   |  | Concurrency (M2)    |
|                  |  |                   |  |                     |
| - UnifiedStore   |  | - WAL             |  | - TransactionContext|
| - BTreeMap       |  | - Recovery        |  | - ClonedSnapshot    |
| - Versioning     |  | - CRC32           |  | - Validation        |
| - TTL cleanup    |  | - 3 sync modes    |  | - Conflict detect   |
+------------------+  +-------------------+  +---------------------+
          |                    |                    |
          +--------------------+--------------------+
                               |
                               v
+-------------------------------------------------------------------------+
|                         Core Types Layer (M1)                            |
|                       (Foundation Definitions)                           |
|                                                                          |
|  Types:                                                                  |
|  - RunId, Namespace, Key, TypeTag, Value, VersionedValue                |
|                                                                          |
|  Traits:                                                                 |
|  - Storage trait (abstraction for store operations)                     |
|  - SnapshotView trait (snapshot isolation)                              |
|                                                                          |
|  Errors:                                                                 |
|  - Error enum (StorageError, ConcurrencyError, etc.)                    |
+-------------------------------------------------------------------------+
```

---

## 2. Primitive Relationships

```
+-------------------------------------------------------------------------+
|                      Primitive Relationships (M3)                        |
+-------------------------------------------------------------------------+

                            Application
                                |
          +---------------------+---------------------+
          |         |          |          |          |
          v         v          v          v          v
     +--------+ +--------+ +--------+ +--------+ +--------+
     |KVStore | |EventLog| | State  | | Trace  | |  Run   |
     |        | |        | | Cell   | | Store  | | Index  |
     +---+----+ +---+----+ +---+----+ +---+----+ +---+----+
         |          |          |          |          |
         |          |          |          |          |
    TypeTag    TypeTag    TypeTag    TypeTag    TypeTag
    = 0x01     = 0x02     = 0x03     = 0x04     = 0x05
         |          |          |          |          |
         +----------+----------+----------+----------+
                               |
                               v
                    +--------------------+
                    |     Database       |
                    | (Transaction API)  |
                    +--------------------+
                               |
                               v
                    +--------------------+
                    |   UnifiedStore     |
                    |    (BTreeMap)      |
                    +--------------------+

Cross-Primitive Transaction Example:
====================================

    db.transaction(run_id, |txn| {
        // KV operation
        txn.kv_put("task/status", "running")?;

        // Event operation
        txn.event_append("task_started", payload)?;

        // State cell operation
        txn.state_cas("workflow", v1, "step2")?;

        // Trace operation
        txn.trace_record(ToolCall { ... })?;

        Ok(())
    })?;

    All four operations are ATOMIC:
    - Either all succeed and commit together
    - Or all fail and roll back together
```

---

## 3. Key Namespace Design

```
+-------------------------------------------------------------------------+
|                         Key Structure (M3)                               |
+-------------------------------------------------------------------------+

Key Format:
===========

    [namespace_bytes] [0x00] [type_tag] [primitive_key]
    |                 |      |          |
    |                 |      |          +-- Primitive-specific key
    |                 |      +------------- Single byte: KV=0x01, Event=0x02, etc.
    |                 +-------------------- Separator byte
    +-------------------------------------- tenant:app:agent:run_id

Examples:
=========

KV Key "config/model":
+---------------------------------------------------------------+
| tenant:app:agent:abc123 | 0x00 | 0x01 | config/model          |
+---------------------------------------------------------------+

Event Key (sequence 42):
+---------------------------------------------------------------+
| tenant:app:agent:abc123 | 0x00 | 0x02 | 0x000000000000002A    |
+---------------------------------------------------------------+
                                           ^-- Big-endian u64

State Key "workflow/status":
+---------------------------------------------------------------+
| tenant:app:agent:abc123 | 0x00 | 0x03 | workflow/status       |
+---------------------------------------------------------------+

Trace Key (UUID):
+---------------------------------------------------------------+
| tenant:app:agent:abc123 | 0x00 | 0x04 | a1b2c3d4-e5f6-...     |
+---------------------------------------------------------------+

Run Key:
+---------------------------------------------------------------+
| tenant:app:agent:*      | 0x00 | 0x05 | run_id_bytes          |
+---------------------------------------------------------------+


Key Ordering Benefits:
=====================

1. Prefix scan for all keys in a namespace:
   SCAN WHERE key STARTS WITH [namespace_bytes]

2. Prefix scan for all keys of a type:
   SCAN WHERE key STARTS WITH [namespace_bytes][0x00][type_tag]

3. Ordered sequence numbers (big-endian):
   Events naturally ordered by sequence
```

---

## 4. KV Store Data Flow

```
+-------------------------------------------------------------------------+
|                      KV Store Operations (M3)                            |
+-------------------------------------------------------------------------+

PUT Operation:
==============

    Application                KVStore              Database           Storage
        |                         |                     |                  |
        |  kv.put(run_id,        |                     |                  |
        |        "key", value)   |                     |                  |
        +------------------------>|                     |                  |
        |                         |                     |                  |
        |                         | transaction(run_id, |                  |
        |                         |   |txn| {...})     |                  |
        |                         +-------------------->|                  |
        |                         |                     |                  |
        |                         |                     | begin_transaction
        |                         |                     |----------------->|
        |                         |                     |   (create snapshot)
        |                         |                     |                  |
        |                         |                     | Build key:       |
        |                         |                     | Key::new_kv(ns, "key")
        |                         |                     |                  |
        |                         |                     | txn.put(key, value)
        |                         |                     |----------------->|
        |                         |                     |  (buffer in write_set)
        |                         |                     |                  |
        |                         |                     | commit_transaction
        |                         |                     |----------------->|
        |                         |                     |  1. Validate     |
        |                         |                     |  2. WAL write    |
        |                         |                     |  3. Apply to store
        |                         |                     |<-----------------|
        |                         |<--------------------|                  |
        |<------------------------|                     |                  |
        |      Ok(())             |                     |                  |


GET Operation:
==============

    Application                KVStore              Database           Storage
        |                         |                     |                  |
        |  kv.get(run_id, "key") |                     |                  |
        +------------------------>|                     |                  |
        |                         |                     |                  |
        |                         | Build key:          |                  |
        |                         | Key::new_kv(ns, key)|                  |
        |                         |                     |                  |
        |                         | db.get(&key)       |                  |
        |                         +-------------------->|                  |
        |                         |                     | storage.get(key) |
        |                         |                     +----------------->|
        |                         |                     |                  |
        |                         |                     |<-----------------|
        |                         |<--------------------|  Some(VersionedValue)
        |<------------------------|                     |                  |
        |  Some(Value)            |                     |                  |


LIST Operation:
===============

    Application                KVStore              Database           Storage
        |                         |                     |                  |
        | kv.list(run_id,        |                     |                  |
        |         Some("prefix"))|                     |                  |
        +------------------------>|                     |                  |
        |                         |                     |                  |
        |                         | Build prefix key:   |                  |
        |                         | Key::new_kv(ns, prefix)               |
        |                         |                     |                  |
        |                         | storage.scan_prefix |                  |
        |                         +-------------------->|                  |
        |                         |                     |----------------->|
        |                         |                     |  BTreeMap range  |
        |                         |                     |<-----------------|
        |                         |<--------------------|                  |
        |<------------------------|                     |                  |
        |  Vec<String> (keys)     |                     |                  |
```

---

## 5. Event Log with Causal Hash Chaining

```
+-------------------------------------------------------------------------+
|                   Event Log Causal Hash Chain (M3)                       |
+-------------------------------------------------------------------------+

IMPORTANT DESIGN NOTES:
=======================
1. EventLog is intentionally SINGLE-WRITER-ORDERED per run
   - All appends are serialized through CAS on metadata key
   - Parallel append is NOT supported

2. Hash chaining is CAUSAL, not cryptographically secure
   - Provides tamper-evidence within process boundary
   - Does NOT provide tamper-resistance at storage level
   - Chain is not externally anchored

Event Structure:
================

    +-------------------------------------------------------------------+
    | Event {                                                            |
    |   sequence: u64,           // Monotonic per run                   |
    |   event_type: String,      // User-defined category               |
    |   payload: Value,          // Event data                          |
    |   timestamp: i64,          // When appended                       |
    |   prev_hash: [u8; 32],     // Hash of previous event              |
    |   hash: [u8; 32],          // This event's hash                   |
    | }                                                                  |
    +-------------------------------------------------------------------+


Chain Structure:
================

    Genesis (seq=0)           Event 1              Event 2              Event 3
    +---------------+     +---------------+     +---------------+     +---------------+
    | seq: 0        |     | seq: 1        |     | seq: 2        |     | seq: 3        |
    | type: _init   |     | type: action  |     | type: result  |     | type: thought |
    | payload: {}   |     | payload: {...}|     | payload: {...}|     | payload: {...}|
    | prev: [0;32]  |     | prev: H0      |     | prev: H1      |     | prev: H2      |
    | hash: H0      |---->| hash: H1      |---->| hash: H2      |---->| hash: H3      |
    +---------------+     +---------------+     +---------------+     +---------------+


Hash Computation:
=================

    fn compute_hash(event, prev_hash) -> [u8; 32]:
        hasher = DefaultHasher::new()
        hasher.update(event.sequence)
        hasher.update(event.event_type)
        hasher.update(event.payload)
        hasher.update(event.timestamp)
        hasher.update(prev_hash)
        return pad_to_32_bytes(hasher.finish())


Append Flow:
============

    Application              EventLog            Database           Storage
        |                       |                    |                  |
        | log.append(run_id,   |                    |                  |
        |   "tool_call", data) |                    |                  |
        +---------------------->|                    |                  |
        |                       |                    |                  |
        |                       | 1. Read metadata   |                  |
        |                       |    (next_seq,      |                  |
        |                       |     head_hash)     |                  |
        |                       +------------------->|                  |
        |                       |                    |----------------->|
        |                       |                    |<-----------------|
        |                       |<-------------------|                  |
        |                       |                    |                  |
        |                       | 2. Create event:   |                  |
        |                       |    seq = next_seq  |                  |
        |                       |    prev = head_hash|                  |
        |                       |    hash = compute()|                  |
        |                       |                    |                  |
        |                       | 3. Transaction:    |                  |
        |                       |    - Store event   |                  |
        |                       |    - Update meta   |                  |
        |                       +------------------->|                  |
        |                       |                    | validate + WAL   |
        |                       |                    | + apply          |
        |                       |<-------------------|                  |
        |<----------------------|                    |                  |
        | (seq, hash)           |                    |                  |


Chain Verification:
===================

    for i in 0..len:
        event = read(i)
        if i == 0:
            assert event.prev_hash == [0; 32]
        else:
            prev_event = read(i - 1)
            assert event.prev_hash == prev_event.hash

        computed = compute_hash(event_data, event.prev_hash)
        assert computed == event.hash
```

---

## 6. StateCell CAS Operations

```
+-------------------------------------------------------------------------+
|                     StateCell CAS Flow (M3)                              |
+-------------------------------------------------------------------------+

WHY "StateCell" NOT "StateMachine":
===================================
In M3, this primitive is a versioned CAS cell - it stores a value with a
version and supports atomic compare-and-swap updates. It does NOT (yet)
enforce allowed transitions, guards, terminal states, or invariants.

A true "StateMachine" with transition definitions may be added in M5+.

State Structure:
================

    +-------------------------------------------+
    | State {                                    |
    |   value: Value,     // Current state      |
    |   version: u64,     // For CAS            |
    |   updated_at: i64,  // Last update time   |
    | }                                          |
    +-------------------------------------------+


CAS Operation Flow:
===================

    Initial State: { value: "pending", version: 5 }

    Thread A                                      Thread B
    =========                                     =========

    1. Read state                                1. Read state
       state = { value: "pending", v: 5 }          state = { value: "pending", v: 5 }

    2. Compute new value                         2. Compute new value
       new = "running"                              new = "cancelled"

    3. CAS(name, expected=5, "running")         3. CAS(name, expected=5, "cancelled")
       |                                            |
       |  Transaction begin                         |
       |  Validate: current.v == 5? YES            |  (waiting for commit lock)
       |  Apply: { "running", v: 6 }               |
       |  Commit                                    |
       |                                            |
       v                                            |
       SUCCESS (returns v: 6)                       |
                                                    v
                                                    Transaction begin
                                                    Validate: current.v == 5?
                                                    NO! (current.v == 6)
                                                    ABORT - CAS conflict


StateCell Usage Pattern:
========================

    // Retry-safe transition
    loop {
        let state = sc.read(run_id, "workflow")?;
        let current = state.value.as_string()?;

        let next = match current.as_str() {
            "pending" => "running",
            "running" => "completed",
            _ => break,
        };

        match sc.cas(run_id, "workflow", state.version, Value::String(next.into())) {
            Ok(_) => break,
            Err(TransactionConflict(_)) => continue,  // Retry
            Err(e) => return Err(e),
        }
    }


Transition Closure Pattern:
===========================

    // Closure may be called multiple times on conflict
    sc.transition(run_id, "counter", |state| {
        let current = state.value.as_i64()?;
        let new_value = Value::I64(current + 1);
        Ok((new_value, current + 1))  // (new_state, return_value)
    })?;

    // Internally:
    // 1. Read state
    // 2. Call closure to compute new value
    // 3. CAS with state.version
    // 4. If conflict, retry from step 1
```

---

## 7. Trace Store with Indices

```
+-------------------------------------------------------------------------+
|                   Trace Store Data Model (M3)                            |
+-------------------------------------------------------------------------+

PERFORMANCE WARNING:
====================
TraceStore is optimized for DEBUGGABILITY, not ingestion throughput.
The 3-4 secondary index entries per trace create write amplification.

Designed for: reasoning traces (tens to hundreds per run)
NOT designed for: telemetry (thousands per second)

For high-volume tracing, consider batching or sampling.

Trace Types:
============

    TraceType::ToolCall { tool_name, arguments, result, duration_ms }
    TraceType::Decision { question, options, chosen, reasoning }
    TraceType::Query { query_type, query, results_count }
    TraceType::Thought { content, confidence }
    TraceType::Error { error_type, message, recoverable }
    TraceType::Custom { trace_type, data }


Storage Layout:
===============

    Primary Key (trace data):
    +---------------------------------------------------------------+
    | namespace:0x04:trace_id -> Trace { id, parent_id, type, ... } |
    +---------------------------------------------------------------+

    Index by Type:
    +---------------------------------------------------------------+
    | namespace:0x04:__idx_type__:ToolCall:trace_id -> ()            |
    | namespace:0x04:__idx_type__:Decision:trace_id -> ()            |
    +---------------------------------------------------------------+

    Index by Tag:
    +---------------------------------------------------------------+
    | namespace:0x04:__idx_tag__:important:trace_id -> ()            |
    | namespace:0x04:__idx_tag__:debug:trace_id -> ()                |
    +---------------------------------------------------------------+

    Index by Parent (for tree structure):
    +---------------------------------------------------------------+
    | namespace:0x04:__idx_parent__:parent_id:child_id -> ()         |
    +---------------------------------------------------------------+

    Index by Time:
    +---------------------------------------------------------------+
    | namespace:0x04:__idx_time__:timestamp_be:trace_id -> ()        |
    +---------------------------------------------------------------+


Hierarchical Traces (Tree Structure):
=====================================

    Root Trace (tool_call)
    +-- id: "abc123"
    |
    +-- Child Trace (thought)
    |   +-- id: "def456"
    |   +-- parent_id: "abc123"
    |
    +-- Child Trace (query)
        +-- id: "ghi789"
        +-- parent_id: "abc123"
        |
        +-- Grandchild Trace (thought)
            +-- id: "jkl012"
            +-- parent_id: "ghi789"


Query Patterns:
===============

    // Query by type - uses __idx_type__ index
    traces.query_by_type(run_id, "ToolCall")?;
    // 1. Scan: namespace:0x04:__idx_type__:ToolCall:*
    // 2. Extract trace_ids from matching keys
    // 3. Fetch full trace data for each id

    // Query by tag - uses __idx_tag__ index
    traces.query_by_tag(run_id, "important")?;
    // 1. Scan: namespace:0x04:__idx_tag__:important:*
    // 2. Extract trace_ids
    // 3. Fetch full trace data

    // Get tree - uses __idx_parent__ index
    traces.get_tree(run_id, "abc123")?;
    // 1. Fetch root trace
    // 2. Scan: namespace:0x04:__idx_parent__:abc123:*
    // 3. Recursively fetch children
```

---

## 8. Run Index Lifecycle

```
+-------------------------------------------------------------------------+
|                      Run Index State Machine                             |
+-------------------------------------------------------------------------+

Run Status Transitions (ENFORCED):
==================================

RunIndex enforces valid status transitions. Invalid transitions return an error.

                    create_run()
                         |
                         v
                    +---------+
            +------>| Active  |<------+
            |       +---------+       |
            |       /    |    \       |
            |      /     |     \      |
            | complete() |  fail()    |
            |    /       |       \    |
            |   v        |        v   |
            | +----------+   +---------+
    resume()|>| Completed|   | Failed  |
            | +----+-----+   +----+----+
            |      |              |
            |      | archive()    | archive()
            |      v              v
            | +----------+   +---------+
            | | Archived |   | Archived|
            | +----------+   +---------+
            |
         pause()
            |
            v
       +---------+
       | Paused  |
       +---------+
            |
        archive() or cancel()
            v
       +---------+       +-----------+
       | Archived| <---- | Cancelled |
       +---------+       +-----------+

VALID TRANSITIONS:
  Active -> Completed, Failed, Cancelled, Paused, Archived
  Paused -> Active, Cancelled, Archived
  Completed -> Archived
  Failed -> Archived
  Cancelled -> Archived

INVALID (will error):
  Completed -> Active (no resurrection)
  Failed -> Active (no resurrection)
  Archived -> * (terminal)
  Failed -> Completed (no retroactive fix)


Run Metadata Storage:
=====================

    +-------------------------------------------------------------------+
    | RunMetadata {                                                      |
    |   run_id: RunId,               // Unique identifier               |
    |   parent_run: Option<RunId>,   // If forked                       |
    |   status: RunStatus,           // Active/Completed/Failed/Archived|
    |   created_at: i64,             // Creation timestamp              |
    |   updated_at: i64,             // Last update timestamp           |
    |   completed_at: Option<i64>,   // When finished                   |
    |   tags: Vec<String>,           // User-defined tags               |
    |   metadata: Value,             // Custom metadata                 |
    |   error: Option<String>,       // Error message if failed         |
    | }                                                                  |
    +-------------------------------------------------------------------+


Index Structure:
================

    Primary Key (run data):
    +---------------------------------------------------------------+
    | namespace:0x05:run_id -> RunMetadata                           |
    +---------------------------------------------------------------+

    Index by Status:
    +---------------------------------------------------------------+
    | namespace:0x05:__idx_status__:Active:run_id -> ()              |
    | namespace:0x05:__idx_status__:Completed:run_id -> ()           |
    +---------------------------------------------------------------+

    Index by Tag:
    +---------------------------------------------------------------+
    | namespace:0x05:__idx_tag__:experiment:run_id -> ()             |
    +---------------------------------------------------------------+

    Index by Parent (for forked runs):
    +---------------------------------------------------------------+
    | namespace:0x05:__idx_parent__:parent_id:child_id -> ()         |
    +---------------------------------------------------------------+


Run Lifecycle Example:
======================

    // 1. Create run
    let run_meta = runs.create_run(&namespace)?;
    // Status: Active
    // Indices created: __idx_status__:Active:run_id

    // 2. Use primitives with this run
    kv.put(run_meta.run_id, "key", value)?;
    log.append(run_meta.run_id, "start", payload)?;

    // 3. Complete run
    runs.complete_run(run_meta.run_id)?;
    // Status: Active -> Completed
    // Index updated:
    //   Remove: __idx_status__:Active:run_id
    //   Add: __idx_status__:Completed:run_id

    // 4. Query completed runs
    let completed = runs.query_runs(RunQuery {
        status: Some(RunStatus::Completed),
        ..Default::default()
    })?;
```

---

## 9. Cross-Primitive Transaction

```
+-------------------------------------------------------------------------+
|                Cross-Primitive Atomic Transaction (M3)                   |
+-------------------------------------------------------------------------+

Scenario: Complete a task atomically
====================================

    Application Code:
    -----------------

    db.transaction(run_id, |txn| {
        // 1. Update KV store
        txn.kv_put("task/123/status", Value::String("completed".into()))?;

        // 2. Append to event log
        txn.event_append("task_completed", json!({
            "task_id": 123,
            "result": "success"
        }))?;

        // 3. Update state cell
        let state = txn.state_read("workflow/progress")?;
        let current = state.map(|s| s.value.as_i64().unwrap_or(0)).unwrap_or(0);
        txn.state_set("workflow/progress", Value::I64(current + 1))?;

        // 4. Record trace
        txn.trace_record(TraceType::Thought {
            content: "Task 123 completed successfully".into(),
            confidence: Some(1.0),
        })?;

        Ok(())
    })?;


Transaction Timeline:
====================

    BEGIN TRANSACTION
    +-- Create snapshot (version V)
    |
    |   Operation 1: KV Put
    |   +-- Build key: namespace:0x01:task/123/status
    |   +-- Add to write_set
    |
    |   Operation 2: Event Append
    |   +-- Read event metadata (from snapshot)
    |   +-- Create event with next sequence
    |   +-- Build key: namespace:0x02:sequence_be
    |   +-- Add event to write_set
    |   +-- Add metadata update to write_set
    |
    |   Operation 3: State Set
    |   +-- Build key: namespace:0x03:workflow/progress
    |   +-- Add to write_set (with new state)
    |
    |   Operation 4: Trace Record
    |   +-- Generate trace_id
    |   +-- Build key: namespace:0x04:trace_id
    |   +-- Add to write_set
    |   +-- Add index entries to write_set
    |
    COMMIT
    +-- Validate all keys in read_set
    +-- Write to WAL (single transaction):
    |   - BeginTxn
    |   - Write (KV)
    |   - Write (Event)
    |   - Write (Event metadata)
    |   - Write (State)
    |   - Write (Trace)
    |   - Write (Trace indices)
    |   - CommitTxn
    +-- Apply all writes to storage atomically
    +-- Update global version


Atomicity Guarantee:
===================

    Either ALL operations succeed:
    +-- KV updated
    +-- Event appended with correct chain
    +-- State cell updated
    +-- Trace recorded with indices

    Or NONE succeed:
    +-- All writes rolled back
    +-- WAL contains no partial transaction
    +-- Storage unchanged
```

---

## 10. Layer Dependencies (M3 Updated)

```
+-------------------------------------------------------------------------+
|                      Dependency Graph (M3)                               |
+-------------------------------------------------------------------------+

                           +----------+
                           |   App    |
                           +----+-----+
                                |
                                | uses all primitives
                                v
    +---------------------------------------------------------------+
    |                     Primitives Layer                           |
    |                                                                |
    | +----------+ +----------+ +----------+ +----------+ +--------+ |
    | | KVStore  | | EventLog | |StateCell | |TraceStore| |RunIndex| |
    | +----+-----+ +----+-----+ +----+-----+ +----+-----+ +---+----+ |
    |      |            |            |            |           |      |
    +------+------------+------------+------------+-----------+------+
           |            |            |            |           |
           +------------+------------+------------+-----------+
                                     |
                           depends on|
                                     v
                           +------------------+
                           |      Engine      |
                           |   (database.rs)  |
                           +--------+---------+
                                    |
              +---------------------+---------------------+
              |                     |                     |
         depends on            depends on            depends on
              |                     |                     |
              v                     v                     v
    +------------------+  +-------------------+  +-------------------+
    |     Storage      |  |    Durability     |  |   Concurrency     |
    |  (unified.rs)    |  |     (wal.rs)      |  |  (transaction.rs) |
    +--------+---------+  +--------+----------+  +--------+----------+
             |                     |                      |
             +---------------------+----------------------+
                                   |
                              depends on
                                   |
                                   v
                           +---------------+
                           |  Core Types   |
                           |  (types.rs)   |
                           +---------------+


New in M3:
==========

    crates/primitives/
    +-- src/
        +-- lib.rs            re-exports all primitives
        +-- kv.rs             KVStore implementation
        +-- event_log.rs      EventLog implementation
        +-- state_cell.rs     StateCell implementation
        +-- trace.rs          TraceStore implementation
        +-- run_index.rs      RunIndex implementation
        +-- extensions.rs     TransactionContext extension traits


Dependency Rules:
=================

    Allowed:
    ---------
    primitives -> engine     (primitives use Database)
    primitives -> core       (primitives use types)
    engine -> storage        (Database uses UnifiedStore)
    engine -> durability     (Database uses WAL)
    engine -> concurrency    (Database uses TransactionContext)
    all -> core              (everyone uses core types)

    Forbidden:
    ----------
    engine -> primitives     (no upward dependencies)
    storage -> engine        (no upward dependencies)
    primitive -> primitive   (no cross-primitive direct deps)


Inter-Primitive Communication:
==============================

    KVStore cannot call EventLog directly.

    Cross-primitive operations go through:
    1. Application layer orchestrates
    2. Database transaction API provides atomicity
    3. Each primitive writes to its own keys
```

---

## 11. TypeTag Namespace

```
+-------------------------------------------------------------------------+
|                      TypeTag Namespace (M3)                              |
+-------------------------------------------------------------------------+

TypeTag Values:
===============

    +--------+------+---------------------------+
    | Name   | Byte | Purpose                   |
    +--------+------+---------------------------+
    | KV     | 0x01 | Key-value store          |
    | Event  | 0x02 | Event log entries        |
    | State  | 0x03 | State cell values        |
    | Trace  | 0x04 | Trace store entries      |
    | Run    | 0x05 | Run index metadata       |
    +--------+------+---------------------------+
    | (reserved for future)                     |
    +--------+------+---------------------------+
    | Vector | 0x10 | Vector store (M6)        |
    +--------+------+---------------------------+


Key Isolation Example:
======================

    Run ID: abc123
    Namespace: tenant:app:agent:abc123

    KV key "config":
    [tenant:app:agent:abc123][0x00][0x01][config]
                                    ^^^^
                                    TypeTag::KV

    Event key (seq 1):
    [tenant:app:agent:abc123][0x00][0x02][0x0000000000000001]
                                    ^^^^
                                    TypeTag::Event

    State key "workflow":
    [tenant:app:agent:abc123][0x00][0x03][workflow]
                                    ^^^^
                                    TypeTag::State


    These keys are DISTINCT even though user portions overlap:
    - KV "workflow" != State "workflow" (different TypeTag)
    - Different runs have different namespace prefixes


BTreeMap Ordering:
==================

    All keys in sorted order:

    tenant:app:agent:run1 | 0x00 | 0x01 | key_a     <- KV
    tenant:app:agent:run1 | 0x00 | 0x01 | key_b     <- KV
    tenant:app:agent:run1 | 0x00 | 0x02 | 00000001  <- Event
    tenant:app:agent:run1 | 0x00 | 0x02 | 00000002  <- Event
    tenant:app:agent:run1 | 0x00 | 0x03 | state_a   <- State
    tenant:app:agent:run2 | 0x00 | 0x01 | key_a     <- Different run!

    Scan for all KV keys in run1:
    PREFIX = tenant:app:agent:run1 | 0x00 | 0x01

    Scan for all events in run1:
    PREFIX = tenant:app:agent:run1 | 0x00 | 0x02
```

---

These diagrams illustrate the key architectural components and flows for M3's Primitives implementation. They build upon M1's storage foundation and M2's transaction layer while adding typed, run-isolated APIs for all five MVP primitives.

**Key Design Points Reflected in These Diagrams**:
- Primitives are logically stateful but operationally stateless
- EventLog is single-writer-ordered with causal (not cryptographic) hash chaining
- StateCell is a CAS cell (not yet a full state machine with transitions)
- TraceStore has write amplification (3-4 index entries per trace)
- RunIndex enforces status transition validity (no resurrection, archived is terminal)
