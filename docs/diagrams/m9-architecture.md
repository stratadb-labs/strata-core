# M9 Architecture Diagrams: API Stabilization & Universal Protocol

This document contains visual representations of the M9 architecture focused on API stabilization, the seven invariants, universal types, and conformance testing.

**Architecture Spec Version**: 1.0

---

## Semantic Invariants (Reference)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         M9 SEMANTIC INVARIANTS                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  THE SEVEN INVARIANTS (FROM PRIMITIVE_CONTRACT.md)                          │
│  ─────────────────────────────────────────────────                          │
│  I1. ADDRESSABLE        Every entity has a stable identity (EntityRef)     │
│  I2. VERSIONED          Every read returns Versioned<T>, writes → Version  │
│  I3. TRANSACTIONAL      All primitives participate in transactions         │
│  I4. LIFECYCLE          Create/exist/evolve/destroy pattern                │
│  I5. RUN-SCOPED         Every entity belongs to exactly one run            │
│  I6. INTROSPECTABLE     exists() and state reads always available          │
│  I7. READ/WRITE         Reads never modify, writes produce versions        │
│                                                                             │
│  THE FOUR ARCHITECTURAL RULES (NON-NEGOTIABLE)                              │
│  ─────────────────────────────────────────────                              │
│  R1. VERSIONED READS    Every read returns Versioned<T>, not raw T         │
│  R2. VERSION RETURNS    Every write returns Version, not ()                │
│  R3. UNIFIED TRAIT      TransactionOps covers ALL 7 primitives             │
│  R4. EXPLICIT SCOPE     RunId always explicit, no ambient context          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. System Architecture Overview (M9)

```
+-------------------------------------------------------------------------+
|                           Application Layer                              |
|                      (Agent Applications using DB)                       |
+-----------------------------------+-------------------------------------+
                                    |
                                    | M9: Universal Protocol
                                    | - Versioned<T> reads
                                    | - Version returns
                                    | - EntityRef addressing
                                    v
+-------------------------------------------------------------------------+
|                          Primitives Layer (M3-M8)                        |
|                          (Stateless Facades)                             |
|                                                                          |
|  M9: All primitives now return Versioned<T> and Version                 |
|                                                                          |
|  +-------------+  +-------------+  +--------------+  +-------------+    |
|  |  KV Store   |  |  Event Log  |  |  StateCell   |  |Trace Store  |    |
|  | Versioned   |  | Versioned   |  | Versioned    |  | Versioned   |    |
|  +------+------+  +------+------+  +------+-------+  +------+------+    |
|         |                |                |                |            |
|         +----------------+-------+--------+----------------+            |
|                                  |                                      |
|  +---------------------------+   |   +-----------------------------+   |
|  |        Run Index          |   |   |      JSON Store (M5)        |   |
|  |        Versioned          |   |   |      Versioned              |   |
|  +-------------+-------------+   |   +-------------+---------------+   |
|                |                 |                 |                    |
|  +─────────────────────────────────────────────────────────────────+   |
|  │                     Vector Store (M8)                            │   |
|  │                     Versioned                                    │   |
|  └─────────────────────────────────────────────────────────────────+   |
+----------------+-----------------+-----------------+--------------------+
                                   |
                                   | M9: TransactionOps trait
                                   | - All 7 primitives unified
                                   | - Consistent API patterns
                                   v
+-------------------------------------------------------------------------+
|                         Engine Layer (M1-M9)                             |
|                   (Orchestration & Coordination)                         |
|                                                                          |
|  +-------------------------------------------------------------------+  |
|  |                          Database                                  |  |
|  |                                                                    |  |
|  |  M9 NEW: Universal Protocol Implementation                        |  |
|  |  +-------------------------------------------------------------+  |  |
|  |  |                   TransactionOps Trait                       |  |  |
|  |  |  - kv_get() → Option<Versioned<Value>>                      |  |  |
|  |  |  - kv_put() → Version                                       |  |  |
|  |  |  - event_append() → Version                                 |  |  |
|  |  |  - state_read() → Option<Versioned<StateValue>>             |  |  |
|  |  |  - json_get() → Option<Versioned<JsonValue>>                |  |  |
|  |  |  - vector_get() → Option<Versioned<VectorEntry>>            |  |  |
|  |  |  - ... all 7 primitives covered                             |  |  |
|  |  +-------------------------------------------------------------+  |  |
|  |                                                                    |  |
|  |  M9 NEW: RunHandle Pattern                                        |  |
|  |  +-------------------------------------------------------------+  |  |
|  |  |                   RunHandle                                  |  |  |
|  |  |  - Ergonomic run-scoped API                                 |  |  |
|  |  |  - fn kv() → KvHandle                                       |  |  |
|  |  |  - fn events() → EventHandle                                |  |  |
|  |  |  - fn transaction<F>(&self, f: F) → Result<T>               |  |  |
|  |  +-------------------------------------------------------------+  |  |
|  |                                                                    |  |
|  +-------------------------------------------------------------------+  |
|                               |                                          |
+----------+-------------------+-------------------+-----------------------+
           |                   |                   |
           v                   v                   v
+------------------+  +-------------------+  +------------------------+
|  Storage (M4+M7) |  | Durability (M4+M7)|  | Concurrency (M4+M9)    |
|                  |  |                   |  |                        |
|                  |  |                   |  | M9 NEW:                |
|                  |  |                   |  | - TransactionOps impl  |
|                  |  |                   |  | - Unified trait        |
|                  |  |                   |  |   across primitives    |
+------------------+  +-------------------+  +------------------------+
           |                   |                   |
           +-------------------+-------------------+
                               |
                               v
+-------------------------------------------------------------------------+
|                         Core Types Layer (M1 + M9)                       |
|                       (Foundation Definitions)                           |
|                                                                          |
|  M9 NEW Types:                                                           |
|  - EntityRef       (universal addressing for all primitives)            |
|  - Versioned<T>    (wrapper with value, version, timestamp)             |
|  - Version         (TxnId | Sequence | Counter)                         |
|  - Timestamp       (microsecond precision)                              |
|  - PrimitiveType   (Kv, Event, State, Trace, Run, Json, Vector)         |
|  - RunId           (standardized across codebase)                       |
|  - StrataError     (unified error type with EntityRef context)          |
+-------------------------------------------------------------------------+
```

---

## 2. The Seven Invariants

```
+-------------------------------------------------------------------------+
|                      The Seven Invariants (M9)                           |
+-------------------------------------------------------------------------+

Invariant 1: Everything is Addressable
======================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Every entity in Strata has a STABLE IDENTITY via EntityRef.        │
    │                                                                     │
    │  pub enum EntityRef {                                               │
    │      Kv { run_id: RunId, key: String },                            │
    │      Event { run_id: RunId, sequence: u64 },                       │
    │      State { run_id: RunId, name: String },                        │
    │      Trace { run_id: RunId, trace_id: TraceId },                   │
    │      Run { run_id: RunId },                                        │
    │      Json { run_id: RunId, doc_id: JsonDocId },                    │
    │      Vector { run_id: RunId, collection: String, vector_id: ... }, │
    │  }                                                                  │
    │                                                                     │
    │  CAPABILITY: Reference, store, pass between systems, retrieve      │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Invariant 2: Everything is Versioned
====================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Every mutation produces a VERSION. Every read returns version info.│
    │                                                                     │
    │  pub struct Versioned<T> {                                          │
    │      pub value: T,                                                  │
    │      pub version: Version,                                          │
    │      pub timestamp: Timestamp,                                      │
    │  }                                                                  │
    │                                                                     │
    │  pub enum Version {                                                 │
    │      TxnId(u64),      // KV, Trace, Run, Vector, Json              │
    │      Sequence(u64),   // EventLog                                  │
    │      Counter(u64),    // StateCell                                 │
    │  }                                                                  │
    │                                                                     │
    │  READ:  fn get(...) → Result<Option<Versioned<Value>>>             │
    │  WRITE: fn put(...) → Result<Version>                              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Invariant 3: Everything is Transactional
========================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ALL primitives participate in transactions THE SAME WAY.           │
    │                                                                     │
    │  db.transaction(run_id, |txn| {                                    │
    │      // KV                                                         │
    │      txn.kv_put("key", value)?;                                    │
    │                                                                     │
    │      // Event                                                       │
    │      txn.event_append("type", payload)?;                           │
    │                                                                     │
    │      // State                                                       │
    │      txn.state_set("cell", value)?;                                │
    │                                                                     │
    │      // Trace                                                       │
    │      txn.trace_record(TraceType::Action, content, tags)?;          │
    │                                                                     │
    │      // Json                                                        │
    │      txn.json_set(doc_id, path, value)?;                           │
    │                                                                     │
    │      // Vector                                                      │
    │      txn.vector_upsert(collection, entries)?;                      │
    │                                                                     │
    │      Ok(())                                                         │
    │  })?;                                                               │
    │                                                                     │
    │  All or nothing: Either ALL operations commit or NONE do.          │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Invariant 4: Everything Has a Lifecycle
=======================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Every entity follows: CREATE → EXIST → EVOLVE → DESTROY            │
    │                                                                     │
    │  ┌────────────┬──────────┬──────────┬──────────┬──────────┐        │
    │  │ Primitive  │  Create  │  Exists  │  Evolve  │ Destroy  │        │
    │  ├────────────┼──────────┼──────────┼──────────┼──────────┤        │
    │  │ KVStore    │   put    │   get    │   put    │  delete  │        │
    │  │ EventLog   │ (implicit)│  read   │  append  │(immutable)│        │
    │  │ StateCell  │   init   │  read    │ set/cas  │  delete  │        │
    │  │ TraceStore │  record  │  read    │(immutable)│(immutable)│       │
    │  │ RunIndex   │create_run│ get_run  │transition│delete_run│        │
    │  │ JsonStore  │  create  │   get    │   set    │ destroy  │        │
    │  │ VectorStore│  upsert  │   get    │  upsert  │  delete  │        │
    │  └────────────┴──────────┴──────────┴──────────┴──────────┘        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Invariant 5: Everything Exists Within a Run
===========================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  All data is SCOPED to a run. Run is the unit of isolation.        │
    │                                                                     │
    │  CORRECT:                              WRONG:                       │
    │  ─────────                             ──────                       │
    │  kv.get(run_id, "key")?;               kv.get("key")?;              │
    │                                        // Where's the run?          │
    │                                                                     │
    │  // Run is always explicit:            // NO ambient context:       │
    │  fn get(&self,                         thread_local! {              │
    │      run_id: &RunId,  ← REQUIRED           static CURRENT_RUN...   │
    │      key: &str,                        }                            │
    │  ) → Result<...>                       // NEVER DO THIS             │
    │                                                                     │
    │  Exception: RunIndex manages runs themselves (meta namespace)       │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Invariant 6: Everything is Introspectable
=========================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Users can always ask: Does it exist? What is its state?           │
    │                                                                     │
    │  Every primitive supports:                                          │
    │  - exists() → bool         // Does the entity exist?               │
    │  - get() → Versioned<T>    // What is its current state + version? │
    │                                                                     │
    │  ┌────────────┬─────────────────────────────────────────────────┐  │
    │  │ Primitive  │ Introspection Method                            │  │
    │  ├────────────┼─────────────────────────────────────────────────┤  │
    │  │ KVStore    │ kv.exists(run_id, key)?                         │  │
    │  │ EventLog   │ events.read(run_id, seq)? → Option<...>         │  │
    │  │ StateCell  │ state.exists(run_id, name)?                     │  │
    │  │ TraceStore │ traces.read(run_id, trace_id)? → Option<...>    │  │
    │  │ RunIndex   │ runs.exists(run_id)?                            │  │
    │  │ JsonStore  │ json.exists(run_id, doc_id)?                    │  │
    │  │ VectorStore│ vectors.get(run_id, coll, key)? → Option<...>   │  │
    │  └────────────┴─────────────────────────────────────────────────┘  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Invariant 7: Reads and Writes Have Consistent Semantics
=======================================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  READS: Never modify state (&self)                                  │
    │  WRITES: Always produce a new version (&mut self) → Version        │
    │                                                                     │
    │  fn get(&self, ...) → Result<Option<Versioned<T>>>  // READ        │
    │  fn put(&mut self, ...) → Result<Version>            // WRITE      │
    │                                                                     │
    │  Within a transaction:                                              │
    │  - Reads see consistent snapshot                                    │
    │  - Reads see prior writes (read-your-writes)                       │
    │  - No operation is "sometimes a read, sometimes a write"           │
    │                                                                     │
    │  ┌────────────────────────────────────────────────────────────────┐│
    │  │ READS (no side effects)  │ WRITES (produce versions)          ││
    │  ├──────────────────────────┼────────────────────────────────────┤│
    │  │ kv.get()                 │ kv.put() → Version                 ││
    │  │ events.read()            │ events.append() → Version          ││
    │  │ state.read()             │ state.set() → Version              ││
    │  │ traces.read()            │ traces.record() → Version          ││
    │  │ json.get()               │ json.set() → Version               ││
    │  │ vectors.get()            │ vectors.upsert() → Version         ││
    │  │ vectors.search()         │                                    ││
    │  └──────────────────────────┴────────────────────────────────────┘│
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 3. The Four Architectural Rules

```
+-------------------------------------------------------------------------+
|                   The Four Architectural Rules (M9)                      |
|                        (NON-NEGOTIABLE)                                  |
+-------------------------------------------------------------------------+

Rule 1: EVERY READ RETURNS Versioned<T>
=======================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  No read operation may return raw values without version info.     │
    │                                                                     │
    │  CORRECT:                              WRONG:                       │
    │  ─────────                             ──────                       │
    │  pub fn get(&self, run_id: &RunId,     pub fn get(&self, run_id,    │
    │      key: &str)                            key: &str)               │
    │  → Result<Option<Versioned<Value>>>    → Result<Option<Value>>      │
    │  {                                     {                            │
    │      // Returns version with value         // NEVER DO THIS        │
    │  }                                     }                            │
    │                                                                     │
    │  WHY: Invariant 2 requires "everything is versioned." If reads     │
    │       don't return versions, users cannot know what version they   │
    │       are looking at.                                              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Rule 2: EVERY WRITE RETURNS Version
===================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Every mutation returns the version it created.                     │
    │                                                                     │
    │  CORRECT:                              WRONG:                       │
    │  ─────────                             ──────                       │
    │  pub fn put(&mut self, run_id,         pub fn put(&mut self, run_id,│
    │      key: &str, value: Value)              key: &str, value: Value) │
    │  → Result<Version>                     → Result<()>                 │
    │  {                                     {                            │
    │      // Returns version created            // NEVER DO THIS        │
    │  }                                     }                            │
    │                                                                     │
    │  WHY: Invariant 2 requires "every mutation produces a version."    │
    │       If writes don't return versions, users cannot track what     │
    │       happened.                                                    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Rule 3: TRANSACTION TRAIT COVERS ALL PRIMITIVES
===============================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Every primitive operation is accessible through TransactionOps.   │
    │                                                                     │
    │  CORRECT:                                                          │
    │  ─────────                                                         │
    │  pub trait TransactionOps {                                        │
    │      // KV                                                         │
    │      fn kv_get(&self, key: &str) → Result<Option<Versioned<Value>>>;
    │      fn kv_put(&mut self, key: &str, value: Value) → Result<Version>;
    │                                                                     │
    │      // Event                                                       │
    │      fn event_append(&mut self, ...) → Result<Version>;            │
    │      fn event_read(&self, seq: u64) → Result<Option<Versioned<Event>>>;
    │                                                                     │
    │      // State                                                       │
    │      fn state_read(&self, name: &str) → Result<Option<Versioned<...>>>;
    │      fn state_set(&mut self, name: &str, ...) → Result<Version>;   │
    │                                                                     │
    │      // Trace                                                       │
    │      fn trace_record(&mut self, ...) → Result<Versioned<TraceId>>; │
    │                                                                     │
    │      // Json                                                        │
    │      fn json_get(&self, ...) → Result<Option<Versioned<JsonValue>>>;
    │      fn json_set(&mut self, ...) → Result<Version>;                │
    │                                                                     │
    │      // Vector                                                      │
    │      fn vector_get(&self, ...) → Result<Option<Versioned<VectorEntry>>>;
    │      fn vector_upsert(&mut self, ...) → Result<Version>;           │
    │  }                                                                  │
    │                                                                     │
    │                                                                     │
    │  WRONG:                                                            │
    │  ──────                                                            │
    │  pub trait TransactionOps {                                        │
    │      fn kv_get(...);                                               │
    │      fn kv_put(...);                                               │
    │      // Missing other primitives! NEVER DO THIS                    │
    │  }                                                                  │
    │                                                                     │
    │  WHY: Invariant 3 requires "every primitive can participate in a   │
    │       transaction." If a primitive isn't in TransactionOps,        │
    │       cross-primitive atomicity breaks.                            │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Rule 4: RUN SCOPE IS ALWAYS EXPLICIT
====================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  The run is always known. No ambient run context.                  │
    │                                                                     │
    │  CORRECT (Handle Pattern):             CORRECT (Parameter):        │
    │  ─────────────────────────             ────────────────────        │
    │  pub struct RunHandle {                pub fn get(&self,           │
    │      run_id: RunId,                        run_id: &RunId,         │
    │      db: Arc<Database>,                    key: &str,              │
    │  }                                     ) → Result<...>;            │
    │                                                                     │
    │  impl RunHandle {                                                   │
    │      pub fn kv(&self) → KvHandle {                                 │
    │          // run_id from self                                       │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    │                                                                     │
    │  WRONG (Ambient Context):                                          │
    │  ────────────────────────                                          │
    │  thread_local! {                                                   │
    │      static CURRENT_RUN: RefCell<Option<RunId>> = ...;            │
    │  }                                                                  │
    │                                                                     │
    │  pub fn get(&self, key: &str) → Result<...> {                     │
    │      let run_id = CURRENT_RUN.with(...);  // NEVER DO THIS        │
    │  }                                                                  │
    │                                                                     │
    │  WHY: Invariant 5 requires "everything exists within a run."       │
    │       If run scope is implicit, it's easy to accidentally cross    │
    │       run boundaries.                                              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 4. Core Types

```
+-------------------------------------------------------------------------+
|                          Core Types (M9)                                 |
+-------------------------------------------------------------------------+

EntityRef: Universal Addressing
===============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  /// Reference to any entity in Strata                             │
    │  /// Expresses Invariant 1: Everything is Addressable              │
    │  pub enum EntityRef {                                               │
    │      Kv { run_id: RunId, key: String },                            │
    │      Event { run_id: RunId, sequence: u64 },                       │
    │      State { run_id: RunId, name: String },                        │
    │      Trace { run_id: RunId, trace_id: TraceId },                   │
    │      Run { run_id: RunId },                                        │
    │      Json { run_id: RunId, doc_id: JsonDocId },                    │
    │      Vector { run_id: RunId, collection: CollectionId,             │
    │               vector_id: VectorId },                               │
    │  }                                                                  │
    │                                                                     │
    │  impl EntityRef {                                                   │
    │      fn run_id(&self) → &RunId { ... }                             │
    │      fn primitive_type(&self) → PrimitiveType { ... }              │
    │  }                                                                  │
    │                                                                     │
    │                                                                     │
    │  Visual: EntityRef Structure                                        │
    │  ───────────────────────────                                        │
    │                                                                     │
    │       EntityRef                                                     │
    │           │                                                         │
    │    ┌──────┴──────────────────────────────────────────────────┐     │
    │    │      │       │       │       │       │       │          │     │
    │    ▼      ▼       ▼       ▼       ▼       ▼       ▼          │     │
    │   Kv   Event   State   Trace   Run    Json   Vector         │     │
    │    │      │       │       │       │       │       │          │     │
    │    │      │       │       │       │       │       │          │     │
    │  run_id run_id  run_id  run_id  run_id  run_id  run_id      │     │
    │  + key  + seq   + name  + id    (only)  + doc_id + coll     │     │
    │                                                   + vec_id  │     │
    │                                                              │     │
    └─────────────────────────────────────────────────────────────────────┘


Versioned<T>: Universal Read Result
===================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  /// Wrapper for any value read from Strata                        │
    │  /// Expresses Invariant 2: Everything is Versioned                │
    │  pub struct Versioned<T> {                                          │
    │      pub value: T,           // The actual data                    │
    │      pub version: Version,   // Version identifier                 │
    │      pub timestamp: Timestamp, // When version was created         │
    │  }                                                                  │
    │                                                                     │
    │  impl<T> Versioned<T> {                                            │
    │      pub fn new(value: T, version: Version, timestamp: Timestamp)  │
    │          → Self;                                                    │
    │                                                                     │
    │      pub fn map<U, F: FnOnce(T) → U>(self, f: F) → Versioned<U> { │
    │          Versioned {                                               │
    │              value: f(self.value),                                 │
    │              version: self.version,                                │
    │              timestamp: self.timestamp,                            │
    │          }                                                          │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    │                                                                     │
    │  Visual: Versioned<T> Structure                                     │
    │  ──────────────────────────────                                     │
    │                                                                     │
    │  ┌─────────────────────────────────────────────────────────────┐   │
    │  │                     Versioned<Value>                         │   │
    │  ├─────────────────────────────────────────────────────────────┤   │
    │  │  value: Value     │  The actual data (KV value, event, etc) │   │
    │  ├───────────────────┼─────────────────────────────────────────┤   │
    │  │  version: Version │  TxnId(42) or Sequence(7) or Counter(3) │   │
    │  ├───────────────────┼─────────────────────────────────────────┤   │
    │  │  timestamp: u64   │  1705123456789012 (microseconds)        │   │
    │  └─────────────────────────────────────────────────────────────┘   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Version: Universal Version Type
===============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  /// Version identifier                                            │
    │  /// Comparable within same entity, not across primitives          │
    │  pub enum Version {                                                 │
    │      TxnId(u64),      // Transaction-based (KV, Trace, Run, etc)  │
    │      Sequence(u64),   // Sequence-based (EventLog)                │
    │      Counter(u64),    // Counter-based (StateCell)                │
    │  }                                                                  │
    │                                                                     │
    │  impl Version {                                                     │
    │      pub fn as_u64(&self) → u64 {                                  │
    │          match self {                                               │
    │              Version::TxnId(v) => *v,                              │
    │              Version::Sequence(v) => *v,                           │
    │              Version::Counter(v) => *v,                            │
    │          }                                                          │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    │                                                                     │
    │  Visual: Version by Primitive                                       │
    │  ────────────────────────────                                       │
    │                                                                     │
    │  ┌────────────┬─────────────────┬───────────────────────────────┐  │
    │  │ Primitive  │ Version Type    │ When Incremented              │  │
    │  ├────────────┼─────────────────┼───────────────────────────────┤  │
    │  │ KVStore    │ TxnId(u64)      │ Each transaction commit       │  │
    │  │ EventLog   │ Sequence(u64)   │ Each event append             │  │
    │  │ StateCell  │ Counter(u64)    │ Each state change             │  │
    │  │ TraceStore │ TxnId(u64)      │ Each trace record             │  │
    │  │ RunIndex   │ TxnId(u64)      │ Each run state change         │  │
    │  │ JsonStore  │ TxnId(u64)      │ Each document mutation        │  │
    │  │ VectorStore│ TxnId(u64)      │ Each vector upsert/delete     │  │
    │  └────────────┴─────────────────┴───────────────────────────────┘  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


PrimitiveType: Primitive Enumeration
====================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  #[derive(Debug, Clone, Copy, PartialEq, Eq)]                      │
    │  pub enum PrimitiveType {                                           │
    │      Kv,                                                            │
    │      Event,                                                         │
    │      State,                                                         │
    │      Trace,                                                         │
    │      Run,                                                           │
    │      Json,                                                          │
    │      Vector,                                                        │
    │  }                                                                  │
    │                                                                     │
    │  Used for:                                                          │
    │  - Runtime type identification                                     │
    │  - Error messages with context                                     │
    │  - Conformance test organization                                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 5. Error Handling

```
+-------------------------------------------------------------------------+
|                        Error Handling (M9)                               |
+-------------------------------------------------------------------------+

StrataError: Unified Error Type
===============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  pub enum StrataError {                                             │
    │      /// Entity not found                                          │
    │      NotFound {                                                     │
    │          entity_ref: EntityRef,  ← Context via EntityRef           │
    │      },                                                             │
    │                                                                     │
    │      /// Version conflict (CAS failure, OCC conflict)              │
    │      VersionConflict {                                              │
    │          entity_ref: EntityRef,                                    │
    │          expected: Version,                                        │
    │          actual: Version,                                          │
    │      },                                                             │
    │                                                                     │
    │      /// Write conflict during transaction                         │
    │      WriteConflict {                                                │
    │          entity_ref: EntityRef,                                    │
    │      },                                                             │
    │                                                                     │
    │      /// Transaction aborted                                       │
    │      TransactionAborted {                                           │
    │          reason: String,                                           │
    │      },                                                             │
    │                                                                     │
    │      /// Run not found                                              │
    │      RunNotFound {                                                  │
    │          run_id: RunId,                                            │
    │      },                                                             │
    │                                                                     │
    │      /// Invalid operation for entity state                        │
    │      InvalidOperation {                                             │
    │          entity_ref: EntityRef,                                    │
    │          reason: String,                                           │
    │      },                                                             │
    │                                                                     │
    │      /// Dimension mismatch (vectors)                              │
    │      DimensionMismatch {                                            │
    │          expected: usize,                                          │
    │          got: usize,                                               │
    │      },                                                             │
    │                                                                     │
    │      /// Collection not found (vectors)                            │
    │      CollectionNotFound {                                           │
    │          run_id: RunId,                                            │
    │          collection: String,                                       │
    │      },                                                             │
    │                                                                     │
    │      /// Storage error                                              │
    │      Storage {                                                      │
    │          message: String,                                          │
    │          source: Option<Box<dyn Error + Send + Sync>>,             │
    │      },                                                             │
    │                                                                     │
    │      /// Serialization error                                       │
    │      Serialization {                                                │
    │          message: String,                                          │
    │      },                                                             │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Error Flow Diagram:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Primitive-specific errors convert to StrataError:                 │
    │                                                                     │
    │  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐         │
    │  │   KvError    │    │  JsonError   │    │ VectorError  │         │
    │  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘         │
    │         │                   │                   │                  │
    │         │   impl From<T>    │   impl From<T>    │   impl From<T>  │
    │         │   for StrataError │   for StrataError │   for StrataError
    │         │                   │                   │                  │
    │         └───────────────────┼───────────────────┘                  │
    │                             │                                      │
    │                             ▼                                      │
    │                    ┌──────────────────┐                           │
    │                    │   StrataError    │                           │
    │                    │                  │                           │
    │                    │ - EntityRef ctx  │                           │
    │                    │ - Structured     │                           │
    │                    │ - Consistent     │                           │
    │                    └──────────────────┘                           │
    │                                                                     │
    │                                                                     │
    │  Example conversion:                                               │
    │  ───────────────────                                               │
    │                                                                     │
    │  impl From<KvError> for StrataError {                              │
    │      fn from(e: KvError) → Self {                                  │
    │          match e {                                                  │
    │              KvError::NotFound { key, run_id } =>                  │
    │                  StrataError::NotFound {                           │
    │                      entity_ref: EntityRef::Kv { run_id, key },   │
    │                  },                                                 │
    │              // ...                                                 │
    │          }                                                          │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 6. Transaction Pattern

```
+-------------------------------------------------------------------------+
|                       Transaction Pattern (M9)                           |
+-------------------------------------------------------------------------+

TransactionOps Trait:
=====================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  /// Operations available within a transaction                     │
    │  /// Expresses Invariant 3: Everything is Transactional            │
    │  pub trait TransactionOps {                                         │
    │                                                                     │
    │      // ═══════════════════════════════════════════════════════    │
    │      // KV Operations                                              │
    │      // ═══════════════════════════════════════════════════════    │
    │      fn kv_get(&self, key: &str)                                   │
    │          → Result<Option<Versioned<Value>>, StrataError>;          │
    │      fn kv_put(&mut self, key: &str, value: Value)                 │
    │          → Result<Version, StrataError>;                           │
    │      fn kv_delete(&mut self, key: &str)                            │
    │          → Result<bool, StrataError>;                              │
    │      fn kv_exists(&self, key: &str)                                │
    │          → Result<bool, StrataError>;                              │
    │                                                                     │
    │      // ═══════════════════════════════════════════════════════    │
    │      // Event Operations                                           │
    │      // ═══════════════════════════════════════════════════════    │
    │      fn event_append(&mut self, event_type: &str, payload: Value)  │
    │          → Result<Version, StrataError>;                           │
    │      fn event_read(&self, sequence: u64)                           │
    │          → Result<Option<Versioned<Event>>, StrataError>;          │
    │      fn event_range(&self, start: u64, end: u64)                   │
    │          → Result<Vec<Versioned<Event>>, StrataError>;             │
    │                                                                     │
    │      // ═══════════════════════════════════════════════════════    │
    │      // State Operations                                           │
    │      // ═══════════════════════════════════════════════════════    │
    │      fn state_read(&self, name: &str)                              │
    │          → Result<Option<Versioned<StateValue>>, StrataError>;     │
    │      fn state_set(&mut self, name: &str, value: Value)             │
    │          → Result<Version, StrataError>;                           │
    │      fn state_cas(&mut self, name: &str, expected: Version,        │
    │                   value: Value)                                    │
    │          → Result<Version, StrataError>;                           │
    │      fn state_delete(&mut self, name: &str)                        │
    │          → Result<bool, StrataError>;                              │
    │      fn state_exists(&self, name: &str)                            │
    │          → Result<bool, StrataError>;                              │
    │                                                                     │
    │      // ═══════════════════════════════════════════════════════    │
    │      // Trace Operations                                           │
    │      // ═══════════════════════════════════════════════════════    │
    │      fn trace_record(&mut self, trace_type: TraceType,             │
    │                      content: Value, tags: Vec<String>)            │
    │          → Result<Versioned<TraceId>, StrataError>;                │
    │      fn trace_read(&self, trace_id: &TraceId)                      │
    │          → Result<Option<Versioned<Trace>>, StrataError>;          │
    │                                                                     │
    │      // ═══════════════════════════════════════════════════════    │
    │      // Json Operations                                            │
    │      // ═══════════════════════════════════════════════════════    │
    │      fn json_create(&mut self, doc_id: &JsonDocId, value: JsonValue)
    │          → Result<Version, StrataError>;                           │
    │      fn json_get(&self, doc_id: &JsonDocId)                        │
    │          → Result<Option<Versioned<JsonValue>>, StrataError>;      │
    │      fn json_get_path(&self, doc_id: &JsonDocId, path: &JsonPath)  │
    │          → Result<Option<Versioned<JsonValue>>, StrataError>;      │
    │      fn json_set(&mut self, doc_id: &JsonDocId, path: &JsonPath,   │
    │                  value: JsonValue)                                 │
    │          → Result<Version, StrataError>;                           │
    │      fn json_delete(&mut self, doc_id: &JsonDocId)                 │
    │          → Result<bool, StrataError>;                              │
    │      fn json_exists(&self, doc_id: &JsonDocId)                     │
    │          → Result<bool, StrataError>;                              │
    │                                                                     │
    │      // ═══════════════════════════════════════════════════════    │
    │      // Vector Operations                                          │
    │      // ═══════════════════════════════════════════════════════    │
    │      fn vector_upsert(&mut self, collection: &CollectionId,        │
    │                       entries: Vec<VectorEntry>)                   │
    │          → Result<Version, StrataError>;                           │
    │      fn vector_get(&self, collection: &CollectionId, id: &VectorId)│
    │          → Result<Option<Versioned<VectorEntry>>, StrataError>;    │
    │      fn vector_delete(&mut self, collection: &CollectionId,        │
    │                       id: &VectorId)                               │
    │          → Result<bool, StrataError>;                              │
    │      fn vector_search(&self, collection: &CollectionId,            │
    │                       query: &[f32], k: usize)                     │
    │          → Result<Vec<VectorMatch>, StrataError>;                  │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Cross-Primitive Transaction Flow:
=================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  db.transaction(&run_id, |txn| {                                   │
    │      ┌───────────────────────────────────────────────────────────┐ │
    │      │                   Transaction Scope                        │ │
    │      │                                                            │ │
    │      │  // 1. Read from KV                                       │ │
    │      │  let config = txn.kv_get("config")?;                      │ │
    │      │                    │                                       │ │
    │      │                    ▼                                       │ │
    │      │  // 2. Append to EventLog                                 │ │
    │      │  let event_v = txn.event_append("config_read", ...)?;     │ │
    │      │                    │                                       │ │
    │      │                    ▼                                       │ │
    │      │  // 3. Update StateCell                                   │ │
    │      │  txn.state_set("last_event", event_v.as_u64())?;          │ │
    │      │                    │                                       │ │
    │      │                    ▼                                       │ │
    │      │  // 4. Record Trace                                       │ │
    │      │  txn.trace_record(TraceType::Action, ...)?;               │ │
    │      │                    │                                       │ │
    │      │                    ▼                                       │ │
    │      │  // 5. Update JSON document                               │ │
    │      │  txn.json_set(&doc_id, &path, value)?;                    │ │
    │      │                    │                                       │ │
    │      │                    ▼                                       │ │
    │      │  // 6. Upsert vector                                      │ │
    │      │  txn.vector_upsert(&collection, entries)?;                │ │
    │      │                                                            │ │
    │      └───────────────────────────────────────────────────────────┘ │
    │                              │                                      │
    │                              ▼                                      │
    │                    ┌────────────────────┐                          │
    │                    │  COMMIT or ABORT   │                          │
    │                    │                    │                          │
    │                    │  Success: ALL 6    │                          │
    │                    │    ops committed   │                          │
    │                    │                    │                          │
    │                    │  Failure: ALL 6    │                          │
    │                    │    ops rolled back │                          │
    │                    └────────────────────┘                          │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 7. RunHandle Pattern

```
+-------------------------------------------------------------------------+
|                       RunHandle Pattern (M9)                             |
+-------------------------------------------------------------------------+

RunHandle: Ergonomic Run-Scoped API
===================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  /// Handle for run-scoped operations                              │
    │  /// Implements Rule 4: Run Scope Is Always Explicit               │
    │  pub struct RunHandle {                                             │
    │      run_id: RunId,                                                │
    │      db: Arc<Database>,                                            │
    │  }                                                                  │
    │                                                                     │
    │  impl RunHandle {                                                   │
    │      /// Access KV primitive                                       │
    │      pub fn kv(&self) → KvHandle;                                  │
    │                                                                     │
    │      /// Access Event primitive                                    │
    │      pub fn events(&self) → EventHandle;                           │
    │                                                                     │
    │      /// Access State primitive                                    │
    │      pub fn state(&self) → StateHandle;                            │
    │                                                                     │
    │      /// Access Trace primitive                                    │
    │      pub fn traces(&self) → TraceHandle;                           │
    │                                                                     │
    │      /// Access Json primitive                                     │
    │      pub fn json(&self) → JsonHandle;                              │
    │                                                                     │
    │      /// Access Vector primitive                                   │
    │      pub fn vectors(&self) → VectorHandle;                         │
    │                                                                     │
    │      /// Execute a transaction                                     │
    │      pub fn transaction<F, T>(&self, f: F) → Result<T>            │
    │      where                                                          │
    │          F: FnOnce(&mut Transaction) → Result<T>;                  │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Usage Pattern:
==============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  // Get or create a run                                            │
    │  let run = db.run(&RunId::new("agent-session-123"));               │
    │                                                                     │
    │  // Access primitives (run is implicit in handle)                  │
    │  let value = run.kv().get("key")?;                                 │
    │  run.kv().put("key", value)?;                                      │
    │                                                                     │
    │  // Transaction within run                                         │
    │  run.transaction(|txn| {                                           │
    │      txn.kv_put("key1", value1)?;                                  │
    │      txn.event_append("log", payload)?;                            │
    │      Ok(())                                                         │
    │  })?;                                                               │
    │                                                                     │
    │                                                                     │
    │  Visual: RunHandle Hierarchy                                        │
    │  ───────────────────────────                                        │
    │                                                                     │
    │                    Database                                         │
    │                       │                                             │
    │             ┌─────────┴─────────┐                                  │
    │             │                   │                                   │
    │             ▼                   ▼                                   │
    │         RunHandle           RunHandle                              │
    │        (run_123)           (run_456)                               │
    │             │                   │                                   │
    │    ┌────┬───┼───┬────┐        ...                                  │
    │    │    │   │   │    │                                              │
    │    ▼    ▼   ▼   ▼    ▼                                             │
    │   KvH  EvH StH TrH JsonH VecH                                      │
    │    │    │   │   │    │    │                                        │
    │    └────┴───┴───┴────┴────┘                                        │
    │              │                                                      │
    │              ▼                                                      │
    │         All scoped to                                              │
    │           run_123                                                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 8. Conformance Testing

```
+-------------------------------------------------------------------------+
|                      Conformance Testing (M9)                            |
+-------------------------------------------------------------------------+

Test Matrix: 7 Primitives × 7 Invariants = 49 Tests
===================================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ┌───────────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────────┐│
    │  │ Primitive │ I1  │ I2  │ I3  │ I4  │ I5  │ I6  │ I7  │ Total   ││
    │  │           │Addr │Ver  │Txn  │Life │Run  │Intro│R/W  │         ││
    │  ├───────────┼─────┼─────┼─────┼─────┼─────┼─────┼─────┼─────────┤│
    │  │ KVStore   │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │   7     ││
    │  │ EventLog  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │   7     ││
    │  │ StateCell │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │   7     ││
    │  │ TraceStore│  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │   7     ││
    │  │ RunIndex  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │   7     ││
    │  │ JsonStore │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │   7     ││
    │  │ VectorStore│ ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │  ✓  │   7     ││
    │  ├───────────┼─────┼─────┼─────┼─────┼─────┼─────┼─────┼─────────┤│
    │  │ Total     │  7  │  7  │  7  │  7  │  7  │  7  │  7  │  49     ││
    │  └───────────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────────┘│
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Test Organization:
==================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  tests/                                                            │
    │  └── conformance/                                                  │
    │      ├── mod.rs                                                    │
    │      │                                                              │
    │      ├── kv/                                                       │
    │      │   ├── invariant_1_addressable.rs                           │
    │      │   ├── invariant_2_versioned.rs                             │
    │      │   ├── invariant_3_transactional.rs                         │
    │      │   ├── invariant_4_lifecycle.rs                             │
    │      │   ├── invariant_5_run_scoped.rs                            │
    │      │   ├── invariant_6_introspectable.rs                        │
    │      │   └── invariant_7_read_write.rs                            │
    │      │                                                              │
    │      ├── event/                                                    │
    │      │   └── (same 7 files)                                       │
    │      │                                                              │
    │      ├── state/                                                    │
    │      │   └── (same 7 files)                                       │
    │      │                                                              │
    │      ├── trace/                                                    │
    │      │   └── (same 7 files)                                       │
    │      │                                                              │
    │      ├── run/                                                      │
    │      │   └── (same 7 files)                                       │
    │      │                                                              │
    │      ├── json/                                                     │
    │      │   └── (same 7 files)                                       │
    │      │                                                              │
    │      ├── vector/                                                   │
    │      │   └── (same 7 files)                                       │
    │      │                                                              │
    │      └── cross_primitive/                                          │
    │          ├── atomicity.rs                                          │
    │          └── isolation.rs                                          │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Example Conformance Tests:
==========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  #[cfg(test)]                                                      │
    │  mod conformance {                                                  │
    │      mod kv {                                                       │
    │          // Invariant 1: Addressable                               │
    │          #[test]                                                    │
    │          fn kv_has_stable_identity() {                             │
    │              let run_id = RunId::new("test");                      │
    │              let ref1 = EntityRef::Kv {                            │
    │                  run_id: run_id.clone(),                           │
    │                  key: "my_key".to_string()                         │
    │              };                                                     │
    │              // ref1 can be stored, passed, used to retrieve       │
    │          }                                                          │
    │                                                                     │
    │          // Invariant 2: Versioned                                 │
    │          #[test]                                                    │
    │          fn kv_reads_return_versioned() {                          │
    │              let result: Option<Versioned<Value>> =                │
    │                  kv.get(&run_id, "key")?;                          │
    │              if let Some(v) = result {                             │
    │                  assert!(matches!(v.version, Version::TxnId(_)));  │
    │              }                                                      │
    │          }                                                          │
    │                                                                     │
    │          #[test]                                                    │
    │          fn kv_writes_return_version() {                           │
    │              let version: Version = kv.put(&run_id, "k", val)?;   │
    │              assert!(matches!(version, Version::TxnId(_)));        │
    │          }                                                          │
    │                                                                     │
    │          // Invariant 3: Transactional                             │
    │          #[test]                                                    │
    │          fn kv_participates_in_cross_primitive_transaction() {     │
    │              db.transaction(&run_id, |txn| {                       │
    │                  txn.kv_put("key", value)?;                        │
    │                  txn.event_append("type", payload)?;               │
    │                  Ok(())                                             │
    │              })?;                                                   │
    │              // Either both committed or neither                   │
    │          }                                                          │
    │                                                                     │
    │          // ... tests for invariants 4-7 ...                       │
    │      }                                                              │
    │                                                                     │
    │      // Same pattern for event, state, trace, run, json, vector   │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Cross-Primitive Tests:
======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  #[test]                                                            │
    │  fn cross_primitive_transaction_atomicity() {                      │
    │      // All 7 primitives in one transaction                        │
    │      let result = db.transaction(&run_id, |txn| {                  │
    │          txn.kv_put("k", v1)?;                                     │
    │          txn.event_append("type", payload)?;                       │
    │          txn.state_set("cell", v2)?;                               │
    │          txn.trace_record(TraceType::Action, content, tags)?;      │
    │          txn.json_set(&doc_id, &path, json_val)?;                  │
    │          txn.vector_upsert(&collection, entries)?;                 │
    │          // Simulate failure here for rollback test               │
    │          Ok(())                                                     │
    │      });                                                            │
    │                                                                     │
    │      // Verify: All committed OR all rolled back                   │
    │  }                                                                  │
    │                                                                     │
    │  #[test]                                                            │
    │  fn cross_primitive_isolation() {                                  │
    │      // Concurrent transactions on different primitives            │
    │      // Verify snapshot isolation                                  │
    │  }                                                                  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 9. Migration Strategy

```
+-------------------------------------------------------------------------+
|                       Migration Strategy (M9)                            |
+-------------------------------------------------------------------------+

Phased Migration:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Phase 1: Add Types (Non-Breaking)                                 │
    │  ─────────────────────────────────                                 │
    │                                                                     │
    │  Add new types without changing existing APIs:                     │
    │  - EntityRef                                                       │
    │  - Versioned<T>                                                    │
    │  - Version                                                         │
    │  - StrataError                                                     │
    │                                                                     │
    │  Existing code continues to work.                                  │
    │                                                                     │
    │                                                                     │
    │  Phase 2: Wrap Returns                                             │
    │  ─────────────────────                                             │
    │                                                                     │
    │  BEFORE:                           AFTER:                          │
    │  ───────                           ──────                          │
    │  fn get(...) → Option<Value>       fn get(...)                     │
    │                                        → Option<Versioned<Value>>  │
    │                                                                     │
    │  fn put(...) → ()                  fn put(...) → Version           │
    │                                                                     │
    │  Deprecate old return types.                                       │
    │                                                                     │
    │                                                                     │
    │  Phase 3: Update Callers                                           │
    │  ────────────────────────                                          │
    │                                                                     │
    │  Update all internal code to use new types:                        │
    │                                                                     │
    │  // Before                                                          │
    │  let value = kv.get(&run_id, "key")?;                              │
    │  process(value);                                                    │
    │                                                                     │
    │  // After                                                           │
    │  let versioned = kv.get(&run_id, "key")?;                          │
    │  if let Some(v) = versioned {                                      │
    │      process(v.value);                                             │
    │      log!("version: {:?}", v.version);                             │
    │  }                                                                  │
    │                                                                     │
    │                                                                     │
    │  Phase 4: Remove Deprecated                                        │
    │  ──────────────────────────                                        │
    │                                                                     │
    │  Remove old API surface after migration complete.                  │
    │  Finalize documentation.                                           │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


API Evolution Timeline:
=======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Timeline:                                                          │
    │  ─────────                                                          │
    │                                                                     │
    │  ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐         │
    │  │ Phase 1 │ →  │ Phase 2 │ →  │ Phase 3 │ →  │ Phase 4 │         │
    │  │ Add     │    │ Wrap    │    │ Update  │    │ Remove  │         │
    │  │ Types   │    │ Returns │    │ Callers │    │ Old API │         │
    │  └─────────┘    └─────────┘    └─────────┘    └─────────┘         │
    │       │              │              │              │               │
    │       │              │              │              │               │
    │       ▼              ▼              ▼              ▼               │
    │  Old API works  Old API works  Migrate tests  API frozen          │
    │  New types avail New returns   Deprecation    M10/M12 ready       │
    │                   + deprecated  warnings                           │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Backwards Compatibility Helper:
===============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  impl<T> Versioned<T> {                                            │
    │      /// Extract just the value, discarding version info           │
    │      ///                                                            │
    │      /// DEPRECATED: Use versioned returns for new code            │
    │      #[deprecated(note = "Use versioned returns directly")]        │
    │      pub fn into_value(self) → T {                                 │
    │          self.value                                                 │
    │      }                                                              │
    │  }                                                                  │
    │                                                                     │
    │  // Migration path for existing code:                              │
    │  // Before: let val = kv.get(...)?.unwrap();                       │
    │  // After:  let val = kv.get(...)?.map(|v| v.into_value());       │
    │  // Final:  let versioned = kv.get(...)?; // Use full info        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 10. Performance Expectations

```
+-------------------------------------------------------------------------+
|                    Performance Expectations (M9)                         |
+-------------------------------------------------------------------------+

M9 is API Changes Only:
=======================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M9 is primarily API changes. Performance impact should be MINIMAL.│
    │                                                                     │
    │  ┌────────────────────────────┬─────────────┬───────────────────┐  │
    │  │        Change              │   Impact    │    Reason         │  │
    │  ├────────────────────────────┼─────────────┼───────────────────┤  │
    │  │ Versioned<T> wrapper       │   < 1%      │ Stack allocation  │  │
    │  │ Version return             │   < 1%      │ Already computed  │  │
    │  │ StrataError                │   0         │ Error path only   │  │
    │  │ TransactionOps trait       │   0         │ Inlined dispatch  │  │
    │  │ EntityRef                  │   < 1%      │ Small struct      │  │
    │  └────────────────────────────┴─────────────┴───────────────────┘  │
    │                                                                     │
    │                                                                     │
    │  Visual: Overhead Analysis                                          │
    │  ─────────────────────────────                                      │
    │                                                                     │
    │  Versioned<T> Memory Layout:                                        │
    │  ┌───────────────────────────────────────────────────────────────┐ │
    │  │  value: T     │  version: Version  │  timestamp: u64         │ │
    │  │  (user data)  │  (8 bytes enum)    │  (8 bytes)              │ │
    │  └───────────────────────────────────────────────────────────────┘ │
    │                                                                     │
    │  Overhead: 16 bytes per read result (stack allocated, no heap)    │
    │  For 1KB value: 16/1024 = 1.5% overhead                           │
    │  For 100B value: 16/100 = 16% size but still < 1% time            │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Non-Regression Requirements:
============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M9 must NOT degrade M7/M8 performance baselines.                  │
    │                                                                     │
    │  ┌────────────────────────────┬─────────────┬───────────────────┐  │
    │  │       Operation            │   Target    │    Red Flag       │  │
    │  ├────────────────────────────┼─────────────┼───────────────────┤  │
    │  │ KV put (InMemory)          │   < 3 µs    │     > 10 µs       │  │
    │  │ KV get (fast path)         │   < 5 µs    │     > 10 µs       │  │
    │  │ Vector upsert              │   < 100 µs  │     > 200 µs      │  │
    │  │ Vector search (k=10)       │   < 10 ms   │     > 20 ms       │  │
    │  │ Snapshot write (100MB)     │   < 5 s     │     > 10 s        │  │
    │  │ Recovery (100MB + 10K WAL) │   < 5 s     │     > 10 s        │  │
    │  └────────────────────────────┴─────────────┴───────────────────┘  │
    │                                                                     │
    │                                                                     │
    │  Run non-regression benchmarks:                                    │
    │  ───────────────────────────────                                   │
    │                                                                     │
    │  ~/.cargo/bin/cargo bench --bench m7_recovery_performance          │
    │  ~/.cargo/bin/cargo bench --bench m8_vector_performance            │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 11. M9 Philosophy

```
+-------------------------------------------------------------------------+
|                           M9 Philosophy                                  |
+-------------------------------------------------------------------------+

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │     M9 is not about features. M9 is about CONTRACTS.                │
    │                                                                     │
    │     Before building the server (M10), before adding Python          │
    │     clients (M12), the interface must be stable. M9 separates       │
    │     invariants from conveniences and substrate from product.        │
    │                                                                     │
    │     "What is the universal way a user interacts with anything       │
    │     in Strata?" This milestone answers that question.               │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M9 IS:
===========

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M9 is an API STABILIZATION milestone.                              │
    │                                                                     │
    │  ✓ Seven invariants documented, enforced, and tested               │
    │  ✓ All seven primitives conform to all seven invariants            │
    │  ✓ Universal types (EntityRef, Versioned<T>, Version) implemented  │
    │  ✓ Unified TransactionOps trait covering all primitives            │
    │  ✓ Consistent StrataError across all primitives                    │
    │  ✓ 49 conformance tests verifying invariant compliance             │
    │  ✓ API consistency audited and verified                            │
    │  ✓ Migration path documented                                       │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M9 is NOT:
===============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M9 is NOT a feature milestone.                                     │
    │                                                                     │
    │  ✗ Wire protocol (M10)                                             │
    │  ✗ Server implementation (M10)                                     │
    │  ✗ Performance optimization (M11)                                  │
    │  ✗ Python SDK (M12)                                                │
    │  ✗ New primitives                                                  │
    │  ✗ Advanced introspection (diff, history)                          │
    │                                                                     │
    │  If a change adds new functionality, it is OUT OF SCOPE.           │
    │  If a change improves consistency without changing semantics,      │
    │  it is IN SCOPE.                                                    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


M9 Locks In:
============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  FROZEN AFTER M9:                                                  │
    │  ─────────────────                                                 │
    │  ✓ The seven invariants (constitutional)                           │
    │  ✓ EntityRef enum structure                                        │
    │  ✓ Versioned<T> wrapper pattern                                    │
    │  ✓ Version enum variants                                           │
    │  ✓ TransactionOps trait method signatures                          │
    │  ✓ StrataError variants                                            │
    │  ✓ RunHandle pattern                                               │
    │                                                                     │
    │  EXTENSIBLE AFTER M9:                                              │
    │  ─────────────────────                                             │
    │  → New EntityRef variants (for new primitives)                     │
    │  → New StrataError variants                                        │
    │  → New TransactionOps methods (additive)                           │
    │  → New Version variants (if needed)                                │
    │                                                                     │
    │  NEVER CHANGE:                                                      │
    │  ─────────────                                                      │
    │  ✗ Existing method signatures in TransactionOps                    │
    │  ✗ Versioned<T> structure                                          │
    │  ✗ The seven invariants                                            │
    │  ✗ The four architectural rules                                    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Success Criteria:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Gate 1: Primitive Contract (Constitutional)                       │
    │  ────────────────────────────────────────────                      │
    │  [ ] Seven invariants documented in PRIMITIVE_CONTRACT.md          │
    │  [ ] All 7 primitives conform to all 7 invariants                  │
    │  [ ] 49 conformance tests pass                                     │
    │                                                                     │
    │  Gate 2: Core API Shape                                            │
    │  ───────────────────────                                           │
    │  [ ] EntityRef type implemented                                    │
    │  [ ] Versioned<T> wrapper for all read operations                  │
    │  [ ] Version type for all write returns                            │
    │  [ ] Unified TransactionOps trait with all primitives              │
    │  [ ] RunHandle pattern implemented                                 │
    │  [ ] StrataError for all error cases                               │
    │                                                                     │
    │  Gate 3: API Consistency Audit                                     │
    │  ─────────────────────────────                                     │
    │  [ ] All reads return Versioned<T>                                 │
    │  [ ] All writes return Version                                     │
    │  [ ] All primitives accessible through same patterns               │
    │  [ ] No primitive-specific special cases in core API               │
    │  [ ] Audit checklist complete                                      │
    │                                                                     │
    │  Gate 4: Documentation                                             │
    │  ─────────────────────                                             │
    │  [ ] PRIMITIVE_CONTRACT.md finalized                               │
    │  [ ] CORE_API_SHAPE.md finalized                                   │
    │  [ ] Migration guide written                                       │
    │                                                                     │
    │  Gate 5: Validation                                                │
    │  ───────────────────                                               │
    │  [ ] Example code works with new API                               │
    │  [ ] Existing tests updated for Versioned<T> returns               │
    │  [ ] Cross-primitive transaction tests pass                        │
    │  [ ] Non-regression benchmarks pass                                │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

These diagrams illustrate the key architectural components and flows for M9's API Stabilization & Universal Protocol milestone. M9 builds upon M7's durability and M8's vector primitive while standardizing the API across all seven primitives.

**Key Design Points Reflected in These Diagrams**:
- Seven invariants from PRIMITIVE_CONTRACT.md define what every primitive must support
- Four architectural rules are non-negotiable: Versioned reads, Version returns, unified TransactionOps, explicit run scope
- EntityRef provides universal addressing for all primitives
- Versioned<T> ensures version information is never lost on reads
- TransactionOps trait unifies all seven primitives under one transaction API
- 49 conformance tests (7 primitives × 7 invariants) verify compliance
- RunHandle pattern provides ergonomic run-scoped API
- StrataError provides unified error handling with EntityRef context

**M9 Philosophy**: M9 is not about features. M9 is about contracts. Before building the server (M10) and Python SDK (M12), the interface must be stable. The seven invariants are constitutional - changes require RFC process, not code changes.
