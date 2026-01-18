# M5 Architecture Diagrams: JSON Primitive

This document contains visual representations of the M5 architecture focused on the native JSON primitive with path-level mutation semantics.

**Architecture Spec Version**: 1.1

---

## Semantic Invariants (Reference)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         M5 SEMANTIC INVARIANTS                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. PATH SEMANTICS ARE POSITIONAL                                           │
│     $.items[0] refers to position, not stable identity.                     │
│     Insertions/removals change what a path refers to.                       │
│                                                                             │
│  2. JSON MUTATIONS ARE PATH-BASED                                           │
│     All writes are mutations to paths, not replacements of blobs.           │
│     WAL records patches, never full documents.                              │
│                                                                             │
│  3. CONFLICT DETECTION IS REGION-BASED                                      │
│     Two operations conflict iff their paths overlap.                        │
│     Overlap = ancestor, descendant, or equal.                               │
│                                                                             │
│  4. WAL IS PATCH-BASED                                                      │
│     Never log full documents.                                               │
│     Patches are idempotent but not reorderable.                             │
│                                                                             │
│  5. JSON PARTICIPATES IN CROSS-PRIMITIVE ATOMICITY                          │
│     Same commit/rollback semantics as KV, Event, State, Trace.              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. System Architecture Overview (M5)

```
+-------------------------------------------------------------------------+
|                           Application Layer                              |
|                      (Agent Applications using DB)                       |
+-----------------------------------+-------------------------------------+
                                    |
                                    | High-level typed APIs
                                    v
+-------------------------------------------------------------------------+
|                          Primitives Layer (M3 + M5)                      |
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
|         +----------------+-------+--------+----------------+            |
|                                  |                                      |
|                                  |                                      |
|  +---------------------------+   |   +-----------------------------+   |
|  |        Run Index          |   |   |      JSON Store (M5 NEW)    |   |
|  |                           |   |   |                             |   |
|  | - create_run()            |   |   | - create()     - cas()      |   |
|  | - get_run()               |   |   | - get()        - version()  |   |
|  | - update_status()         |   |   | - set()        - patch()    |   |
|  | - query_runs()            |   |   | - delete()     - exists()   |   |
|  +-------------+-------------+   |   +-------------+---------------+   |
|                |                 |                 |                    |
+----------------+-----------------+-----------------+--------------------+
                                   |
                                   | Database transaction API
                                   v
+-------------------------------------------------------------------------+
|                         Engine Layer (M1-M2-M4)                          |
|                   (Orchestration & Coordination)                         |
|                                                                          |
|  +-------------------------------------------------------------------+  |
|  |                          Database                                  |  |
|  |                                                                    |  |
|  |  Transaction Context (M5 Extension):                              |  |
|  |  - json_read_set: Option<Vec<JsonReadEntry>>   (lazy init)        |  |
|  |  - json_write_set: Option<Vec<JsonWriteEntry>> (lazy init)        |  |
|  |                                                                    |  |
|  |  M5 NEW: Path-based conflict detection                            |  |
|  |  - Region overlap checking                                         |  |
|  |  - Document-level versioning                                       |  |
|  |                                                                    |  |
|  +-------------------------------------------------------------------+  |
|                               |                                          |
+----------+-------------------+-------------------+-----------------------+
           |                   |                   |
           v                   v                   v
+------------------+  +-------------------+  +------------------------+
|  Storage (M4)    |  | Durability (M4)   |  | Concurrency (M4+M5)    |
|                  |  |     + M5 WAL      |  |                        |
|                  |  |                   |  |                        |
| - ShardedStore   |  | - InMemoryMode    |  | - Transaction Pooling  |
| - DashMap        |  | - BufferedMode    |  | - Read Fast Path       |
| - TypeTag::Json  |  | - StrictMode      |  | - JSON Read/Write Sets |
|   = 0x06 (NEW)   |  | - JSON WAL Entries|  | - Path Overlap Check   |
+------------------+  +-------------------+  +------------------------+
           |                   |                   |
           +-------------------+-------------------+
                               |
                               v
+-------------------------------------------------------------------------+
|                         Core Types Layer (M1 + M5)                       |
|                       (Foundation Definitions)                           |
|                                                                          |
|  M5 NEW Types:                                                           |
|  - JsonDocId     (document identity)                                    |
|  - JsonPath      (path into document)                                   |
|  - JsonValue     (structured value)                                     |
|  - JsonPatch     (mutation operation)                                   |
|  - JsonWalEntry  (WAL format)                                           |
+-------------------------------------------------------------------------+
```

---

## 2. JSON Document Model

```
+-------------------------------------------------------------------------+
|                       JSON Document Model (M5)                           |
+-------------------------------------------------------------------------+

Document Structure:
===================

    +-------------------------------------------------------------------+
    | JsonDoc {                                                          |
    |   doc_id: JsonDocId(Uuid),     // Unique identifier               |
    |   data: Vec<u8>,               // Serialized JSON (msgpack)       |
    |   version: u64,                // Single doc version              |
    |   created_at: Timestamp,       // Creation time                   |
    |   modified_at: Timestamp,      // Last modification               |
    | }                                                                  |
    +-------------------------------------------------------------------+


M5 Storage Format (Simple Blob):
================================

    +-------------------------------------------------------------------+
    |                                                                    |
    |  M5 DOES NOT use structural representation.                       |
    |  M5 uses simple blob format:                                      |
    |                                                                    |
    |    get(path):  deserialize → traverse → return value              |
    |    set(path):  deserialize → modify → serialize → store           |
    |    delete(path): deserialize → remove → serialize → store         |
    |                                                                    |
    |  This is intentionally simple. Structural storage is M6+.         |
    |                                                                    |
    +-------------------------------------------------------------------+


Example Document:
================

    JsonDocId: "a1b2c3d4-..."

    Logical Structure:
    {
      "user": {
        "name": "Alice",
        "settings": {
          "theme": "dark",
          "notifications": true
        }
      },
      "items": [
        { "id": 1, "price": 100 },
        { "id": 2, "price": 200 }
      ]
    }

    Storage:
    +---------------------------------------------------------------+
    | namespace:0x06:a1b2c3d4-... -> [msgpack bytes] + version: 7   |
    +---------------------------------------------------------------+


Document Lifecycle:
===================

    create(initial_value)
           |
           v
    +---------------+
    |   Document    |
    |   version: 1  |
    +---------------+
           |
           | get/set/delete/patch operations
           | (version increments on every change)
           v
    +---------------+
    |   Document    |
    |   version: N  |
    +---------------+
           |
           | delete_document()
           v
    +---------------+
    |   (removed)   |
    +---------------+
```

---

## 3. Path Semantics

```
+-------------------------------------------------------------------------+
|                         Path Semantics (M5)                              |
+-------------------------------------------------------------------------+

Path Structure:
===============

    JsonPath {
        segments: Vec<PathSegment>
    }

    PathSegment:
    - Key(String)   → object property access: .foo
    - Index(usize)  → array element access: [0]


Path Examples:
==============

    Path String        Segments                 Meaning
    ─────────────────────────────────────────────────────────────────────
    ""                 []                       Root (entire doc)
    ".user"            [Key("user")]            Object property
    ".user.name"       [Key("user"),            Nested property
                        Key("name")]
    ".items[0]"        [Key("items"),           Array element
                        Index(0)]
    ".items[0].price"  [Key("items"),           Nested in array
                        Index(0),
                        Key("price")]


CRITICAL: Paths Are Positional, Not Identity-Based
=================================================

    Before insert:
    $.items = [A, B, C]

    $.items[0] → A
    $.items[1] → B
    $.items[2] → C

    After insert(0, X):
    $.items = [X, A, B, C]

    $.items[0] → X     ← CHANGED!
    $.items[1] → A     ← CHANGED!
    $.items[2] → B     ← CHANGED!
    $.items[3] → C     ← CHANGED!

    +-------------------------------------------------------------------+
    |                                                                    |
    |  PATHS ARE VIEWS, NOT REFERENCES                                  |
    |                                                                    |
    |  Structural modifications invalidate previously read paths.       |
    |  This is why M5 does not provide array_insert/array_remove.       |
    |                                                                    |
    +-------------------------------------------------------------------+


Path Overlap Detection:
=======================

    fn overlaps(a: &JsonPath, b: &JsonPath) -> bool {
        a.is_ancestor_of(b) || a.is_descendant_of(b) || a == b
    }

    Examples:

    +-------------------+-------------------+-----------+
    |    Path A         |    Path B         | Overlaps? |
    +-------------------+-------------------+-----------+
    | $.user.name       | $.user.email      |    NO     |  siblings
    | $.user            | $.user.name       |    YES    |  ancestor
    | $.user.name       | $.user            |    YES    |  descendant
    | $.items[0]        | $.items[1]        |    NO     |  siblings
    | $.items           | $.items[0]        |    YES    |  ancestor
    | $                 | $.anything        |    YES    |  root overlaps all
    | $.a               | $.b               |    NO     |  disjoint subtrees
    +-------------------+-------------------+-----------+
```

---

## 4. Conflict Detection

```
+-------------------------------------------------------------------------+
|                      Conflict Detection (M5)                             |
+-------------------------------------------------------------------------+

Region-Based Conflict Detection:
================================

    Two JSON operations conflict if and only if:
    1. They target the SAME document, AND
    2. Their paths OVERLAP

    fn operations_conflict(op1: &JsonOp, op2: &JsonOp) -> bool {
        op1.doc_id == op2.doc_id && op1.path.overlaps(&op2.path)
    }


Conflict Matrix:
================

    +---------------------------+---------------------------+-----------+
    |         Write A           |         Write B           | Conflict? |
    +---------------------------+---------------------------+-----------+
    | doc1: $.user.name         | doc1: $.user.email        |    NO     |
    | doc1: $.user.name         | doc1: $.user.name         |    YES    |
    | doc1: $.user              | doc1: $.user.name         |    YES    |
    | doc1: $.user.name         | doc1: $.user              |    YES    |
    | doc1: $.x                 | doc1: $.y                 |    NO     |
    | doc1: $                   | doc1: $.anything          |    YES    |
    | doc1: $.user              | doc2: $.user              |    NO     |
    +---------------------------+---------------------------+-----------+


Visual: Non-Conflicting Writes
==============================

    Transaction A                    Transaction B
    writes: $.user.name              writes: $.user.email

        {
          "user": {
            "name": ←────── A writes here
            "email": ←────── B writes here
          }
        }

    Result: BOTH COMMIT (paths don't overlap)


Visual: Conflicting Writes
==========================

    Transaction A                    Transaction B
    writes: $.user                   writes: $.user.name

        {
          "user": ←────── A writes entire subtree
            {
              "name": ←────── B writes here (inside A's region)
            }
        }

    Result: ONE ABORTS (paths overlap - A is ancestor of B)


Array Mutations Are Conservative:
=================================

    +-------------------------------------------------------------------+
    |                                                                    |
    |  Any array mutation conflicts with ALL paths through that array.  |
    |                                                                    |
    |  Why? Because paths are positional. An insert at [0] changes      |
    |  what [1], [2], [3], etc. refer to.                              |
    |                                                                    |
    +-------------------------------------------------------------------+

    Example:

    Transaction A: insert($.items, 0, X)
    Transaction B: set($.items[1].price, 500)

    CONFLICT! Because:
    - A modifies array structure
    - B reads/writes through array
    - After A's insert, B's [1] refers to different element


M5 Document-Level Version Check:
================================

    +-------------------------------------------------------------------+
    |                                                                    |
    |  M5 LIMITATION: Document-level versioning                         |
    |                                                                    |
    |  If ANY part of document changes after snapshot,                  |
    |  ALL reads from that document fail validation.                    |
    |                                                                    |
    |  This is conservative but correct.                                |
    |  M6+ with per-path versioning can be more precise.                |
    |                                                                    |
    +-------------------------------------------------------------------+

    Timeline:

    T1: Txn A takes snapshot (doc version = 5)
    T2: Txn A reads $.user.name
    T3: Txn B commits write to $.items[0] (doc version → 6)
    T4: Txn A tries to commit

    Result: Txn A FAILS (doc version changed from 5 to 6)

    Even though $.user.name and $.items[0] don't overlap!
    (This is the M5 limitation we accept for simplicity)
```

---

## 5. Versioning Model

```
+-------------------------------------------------------------------------+
|                        Versioning Model (M5)                             |
+-------------------------------------------------------------------------+

Document-Granular Versioning:
=============================

    Each document has ONE version number.
    No per-path or per-node versions in M5.

    +-------------------+           +-------------------+
    | JsonDoc           |           | JsonDoc           |
    | version: 5        |  ──set──> | version: 6        |
    |                   |           |                   |
    | { user: {...},    |           | { user: {...},    |
    |   items: [...] }  |           |   items: [...] }  |
    +-------------------+           +-------------------+

    ANY change increments the document version.


Why NOT Per-Path Versioning in M5?
==================================

    Per-path versioning requires:

    ┌─────────────────────────────────────────────────────────────────┐
    │                                                                 │
    │  1. Version propagation rules                                   │
    │     - Which ancestors get updated when a leaf changes?          │
    │                                                                 │
    │  2. Version dominance rules                                     │
    │     - Parent has v5, child has v7 - which wins for reads?       │
    │                                                                 │
    │  3. Snapshot read rules                                         │
    │     - How to find the version of each subtree at snapshot time? │
    │                                                                 │
    │  4. Partial ordering semantics                                  │
    │     - What if sibling subtrees have incomparable versions?      │
    │                                                                 │
    │  5. Read-your-writes across version boundaries                  │
    │     - Complex interaction with write buffering                  │
    │                                                                 │
    └─────────────────────────────────────────────────────────────────┘

    Each is a research-grade problem.
    M5 avoids this complexity.


Version Allocation:
===================

    JSON versions come from the global version counter:

    ┌──────────────────────────────────────────────────┐
    │                Global Version Counter            │
    │                                                  │
    │   Used by: KV, Event, State, Trace, Run, JSON   │
    │                                                  │
    │   Ensures global ordering across all primitives │
    └──────────────────────────────────────────────────┘

    Example:

    v100: KV put
    v101: JSON create (doc1, version=101)
    v102: Event append
    v103: JSON set (doc1, version=103)
    v104: KV delete


Trade-off:
==========

    +-------------------+------------------------------------------+
    |   More false      |   Simpler and                            |
    |   conflicts       |   provably correct                       |
    +-------------------+------------------------------------------+

    M5 accepts false conflicts for simplicity.
    M6+ can add per-path versioning for precision.
```

---

## 6. Snapshot Semantics

```
+-------------------------------------------------------------------------+
|                       Snapshot Semantics (M5)                            |
+-------------------------------------------------------------------------+

M5 Provides WEAK Snapshot Isolation:
====================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Reads see a consistent view of documents that have NOT been        │
    │  modified since the snapshot was taken.                             │
    │                                                                     │
    │  If a document IS modified after snapshot, reads FAIL.              │
    │  (Not stale reads - explicit failure)                               │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Why Weak Snapshots?
===================

    Full snapshot isolation requires:

    +-------------------------------------------------------------------+
    | Option 1: Version chains (MVCC)                                   |
    |   Keep old versions, read at snapshot version                     |
    |   → Requires structural representation + GC                       |
    +-------------------------------------------------------------------+
    | Option 2: Copy-on-write subtrees                                  |
    |   Clone subtrees on modification                                  |
    |   → Requires structural representation + sharing                  |
    +-------------------------------------------------------------------+
    | Option 3: Structural persistence                                  |
    |   Persistent data structures with path copying                    |
    |   → Requires structural representation + immutability             |
    +-------------------------------------------------------------------+

    All require structural representation → M6+


M5 Weak Snapshot Behavior:
==========================

    Timeline:

    T1: Txn A starts, snapshot version = 10
    T2: Txn A reads doc1 (doc1.version = 8, OK: 8 ≤ 10)
    T3: Txn B commits, modifies doc1 (doc1.version → 11)
    T4: Txn A reads doc1 again

    Result at T4:

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  FULL MVCC (not M5):  Returns doc1 at version 8 (historical read)  │
    │                                                                     │
    │  M5 WEAK SNAPSHOT:    Returns ERROR (doc modified after snapshot)  │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Error Handling Pattern:
=======================

    // Application code
    let result = db.transaction(run_id, |txn| {
        let value = txn.json_get(doc_id, &path)?;
        // ... use value ...
        Ok(value)
    });

    match result {
        Ok(v) => { /* success */ }
        Err(Error::Conflict(_)) => {
            // Document was modified - retry transaction
        }
        Err(e) => { /* other error */ }
    }

    Standard OCC retry pattern applies.


What M5 Guarantees:
===================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ✓ No dirty reads (uncommitted changes invisible)                   │
    │  ✓ No torn reads (partial document states impossible)               │
    │  ✓ Read-your-writes (within same transaction)                       │
    │  ✓ Explicit failure (never silent stale reads)                      │
    │                                                                     │
    │  ✗ Historical reads (cannot read old versions)                      │
    │  ✗ Time travel (cannot query past states)                           │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 7. WAL Integration

```
+-------------------------------------------------------------------------+
|                        WAL Integration (M5)                              |
+-------------------------------------------------------------------------+

Patch-Based WAL Entries:
========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  WAL entries record PATCHES, not full documents.                    │
    │  This is a NON-NEGOTIABLE semantic decision.                        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    enum JsonWalEntry {
        Create  { run_id, doc_id, value, version, timestamp },
        Set     { run_id, doc_id, path, value, version },
        Delete  { run_id, doc_id, path, version },
        DeleteDoc { run_id, doc_id, version },
    }


WAL Entry Type Tags:
====================

    Existing WAL tags:           New JSON tags (0x20 range):
    ─────────────────            ────────────────────────────
    0x01 = BEGIN_TXN             0x20 = JSON_CREATE
    0x02 = WRITE                 0x21 = JSON_SET
    0x03 = DELETE                0x22 = JSON_DELETE
    0x04 = COMMIT_TXN            0x23 = JSON_DELETE_DOC
    0x05 = ABORT_TXN
    0x06 = CHECKPOINT


Why Patch-Based?
================

    Scenario: 1MB document, change one field

    +-------------------+-------------------+
    | Full-Document WAL | Patch-Based WAL   |
    +-------------------+-------------------+
    | Write ~1 MB       | Write ~100 bytes  |
    | (entire doc)      | (path + value)    |
    +-------------------+-------------------+

    Patch-based is 10,000× more efficient.


WAL Entry Example:
==================

    Transaction: Set $.user.name = "Bob" on doc a1b2c3d4

    WAL Entry:
    +-------------------------------------------------------------------+
    | 0x21 (JSON_SET)                                                    |
    | run_id: [16 bytes]                                                |
    | doc_id: a1b2c3d4-e5f6-...                                         |
    | path: [Key("user"), Key("name")]                                  |
    | value: "Bob"                                                      |
    | version: 42                                                       |
    +-------------------------------------------------------------------+


Idempotent Replay:
==================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Patches must be SAFELY REPLAYABLE if applied twice.               │
    │                                                                     │
    │  Idempotence check: if doc.version >= entry.version, skip.         │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    fn replay_json_entry(entry: &JsonWalEntry) -> Result<()> {
        match entry {
            JsonWalEntry::Set { doc_id, version, .. } => {
                if let Some(doc) = storage.get(doc_id)? {
                    if doc.version >= *version {
                        return Ok(());  // Already applied
                    }
                }
                // Apply the entry...
            }
            // ...
        }
    }


WAL Ordering:
=============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  JSON WAL entries are NOT commutative.                              │
    │  They MUST be replayed in exact order.                              │
    │                                                                     │
    │  Idempotent: YES (safe to apply twice)                              │
    │  Reorderable: NO (order matters)                                    │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    Example:

    Entry 1: Set($.user, {})
    Entry 2: Set($.user.name, "Alice")

    Correct order:  $.user = {}, then $.user.name = "Alice"
                    → $.user = { "name": "Alice" }

    Wrong order:    $.user.name = "Alice" (fails: $.user doesn't exist)
                    → Error!
```

---

## 8. Transaction Integration

```
+-------------------------------------------------------------------------+
|                     Transaction Integration (M5)                         |
+-------------------------------------------------------------------------+

Transaction Context Extension:
==============================

    impl TransactionContext {
        // Existing fields (unchanged)...

        // M5 NEW: JSON tracking (LAZY - only allocated when needed)
        json_read_set: Option<Vec<JsonReadEntry>>,
        json_write_set: Option<Vec<JsonWriteEntry>>,
    }


Lazy Activation (Zero Overhead for Non-JSON):
=============================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  Non-JSON transactions pay ZERO overhead for JSON support.          │
    │                                                                     │
    │  JSON tracking is only allocated when JSON operations occur.        │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    fn json_get(&mut self, doc_id, path) -> Result<...> {
        // Lazy init - first JSON op allocates tracking
        if self.json_read_set.is_none() {
            self.json_read_set = Some(Vec::new());
            self.json_write_set = Some(Vec::new());
        }
        // ... proceed with operation
    }


Read-Your-Writes:
=================

    fn json_get(&mut self, doc_id, path) -> Result<Option<JsonValue>> {

        // 1. Check write set first (read-your-writes)
        if let Some(write_set) = &self.json_write_set {
            for entry in write_set.iter().rev() {
                if entry.doc_id == *doc_id && entry.path.is_ancestor_of(path) {
                    return self.resolve_from_write(entry, path);
                }
            }
        }

        // 2. Read from snapshot (not found in writes)
        let result = self.snapshot.json_get(doc_id, path)?;

        // 3. Track read for conflict detection
        self.json_read_set.as_mut().unwrap().push(JsonReadEntry {
            doc_id: *doc_id,
            path: path.clone(),
            doc_version: self.snapshot.get_json_doc_version(doc_id)?,
        });

        Ok(result)
    }


Commit Validation:
==================

    fn validate_json(&self, storage: &Storage) -> Result<(), ConflictError> {
        let Some(read_set) = &self.json_read_set else {
            return Ok(());  // No JSON operations
        };

        for read in read_set {
            let current_version = storage.get_json_doc_version(&read.doc_id)?;

            if current_version != read.doc_version {
                // Document was modified - conflict!
                return Err(ConflictError::JsonConflict {
                    doc_id: read.doc_id,
                    read_version: read.doc_version,
                    current_version,
                });
            }
        }

        Ok(())
    }


Cross-Primitive Transaction:
============================

    db.transaction(run_id, |txn| {
        // KV operation
        txn.kv_put("config", Value::String("enabled".into()))?;

        // JSON operation
        let doc_id = txn.json_create(json!({ "status": "active" }))?;
        txn.json_set(doc_id, &path("$.count"), json!(0))?;

        // Event operation
        txn.event_append("doc_created", json!({ "doc_id": doc_id }))?;

        Ok(doc_id)
    })?;

    // All commit atomically or none do!

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  JSON operations participate in the same atomicity guarantees       │
    │  as KV, Event, State, Trace, and Run operations.                   │
    │                                                                     │
    │  - Atomic commit                                                    │
    │  - Atomic rollback                                                  │
    │  - Conflict-driven aborts                                           │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

## 9. Data Flow: JSON Operations

```
+-------------------------------------------------------------------------+
|                   M5 Data Flow: JSON Create Operation                    |
+-------------------------------------------------------------------------+

    Application           JsonStore            Database          ShardedStore
        │                     │                    │                   │
        │ json.create(        │                    │                   │
        │   run_id,           │                    │                   │
        │   json!({"a": 1}))  │                    │                   │
        ├────────────────────►│                    │                   │
        │                     │                    │                   │
        │                     │ transaction(...)   │                   │
        │                     ├───────────────────►│                   │
        │                     │                    │                   │
        │                     │                    │ Generate doc_id   │
        │                     │                    │ (UUID::new_v4)    │
        │                     │                    │                   │
        │                     │                    │ Serialize to      │
        │                     │                    │ msgpack           │
        │                     │                    │                   │
        │                     │                    │ Build key:        │
        │                     │                    │ ns:0x06:doc_id    │
        │                     │                    │                   │
        │                     │                    │ Add to write_set  │
        │                     │                    │                   │
        │                     │                    │ commit()          │
        │                     │                    ├──────────────────►│
        │                     │                    │                   │
        │                     │                    │ WAL: JSON_CREATE  │
        │                     │                    │ Apply to storage  │
        │                     │                    │                   │
        │                     │◄───────────────────┤                   │
        │◄────────────────────┤                    │                   │
        │  Ok(JsonDocId)      │                    │                   │


+-------------------------------------------------------------------------+
|                   M5 Data Flow: JSON Get Operation                       |
+-------------------------------------------------------------------------+

    Application           JsonStore            Database          ShardedStore
        │                     │                    │                   │
        │ json.get(           │                    │                   │
        │   run_id,           │                    │                   │
        │   doc_id,           │                    │                   │
        │   &path("$.a.b"))   │                    │                   │
        ├────────────────────►│                    │                   │
        │                     │                    │                   │
        │                     │ Build key:         │                   │
        │                     │ ns:0x06:doc_id     │                   │
        │                     │                    │                   │
        │                     │ storage.get(key)   │                   │
        │                     ├────────────────────────────────────────►│
        │                     │                    │                   │
        │                     │◄────────────────────────────────────────┤
        │                     │  JsonDoc bytes     │                   │
        │                     │                    │                   │
        │                     │ Deserialize        │                   │
        │                     │ msgpack → JSON     │                   │
        │                     │                    │                   │
        │                     │ Traverse path      │                   │
        │                     │ $.a.b              │                   │
        │                     │                    │                   │
        │◄────────────────────┤                    │                   │
        │  Ok(Some(value))    │                    │                   │


+-------------------------------------------------------------------------+
|                   M5 Data Flow: JSON Set Operation                       |
+-------------------------------------------------------------------------+

    Application           JsonStore            Database          ShardedStore
        │                     │                    │                   │
        │ json.set(           │                    │                   │
        │   run_id,           │                    │                   │
        │   doc_id,           │                    │                   │
        │   &path("$.a.b"),   │                    │                   │
        │   json!(42))        │                    │                   │
        ├────────────────────►│                    │                   │
        │                     │                    │                   │
        │                     │ transaction(...)   │                   │
        │                     ├───────────────────►│                   │
        │                     │                    │                   │
        │                     │                    │ 1. Read current   │
        │                     │                    │    doc bytes      │
        │                     │                    │                   │
        │                     │                    │ 2. Deserialize    │
        │                     │                    │                   │
        │                     │                    │ 3. Navigate to    │
        │                     │                    │    $.a.b          │
        │                     │                    │                   │
        │                     │                    │ 4. Update value   │
        │                     │                    │                   │
        │                     │                    │ 5. Serialize      │
        │                     │                    │                   │
        │                     │                    │ 6. Increment ver  │
        │                     │                    │                   │
        │                     │                    │ commit()          │
        │                     │                    ├──────────────────►│
        │                     │                    │                   │
        │                     │                    │ WAL: JSON_SET     │
        │                     │                    │ (patch, not blob) │
        │                     │                    │                   │
        │                     │◄───────────────────┤                   │
        │◄────────────────────┤                    │                   │
        │  Ok(())             │                    │                   │
```

---

## 10. TypeTag Namespace (M5 Update)

```
+-------------------------------------------------------------------------+
|                    TypeTag Namespace (M3 + M5)                           |
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
    | Json   | 0x06 | JSON documents (M5 NEW)  |
    +--------+------+---------------------------+
    | (reserved for future)                     |
    +--------+------+---------------------------+
    | Vector | 0x10 | Vector store (M8)        |
    +--------+------+---------------------------+


Key Structure:
==============

    JSON document key:
    +---------------------------------------------------------------+
    | namespace:0x06:doc_id_bytes                                    |
    +---------------------------------------------------------------+
      ^                ^     ^
      |                |     |
      |                |     +-- Document UUID
      |                +-------- TypeTag::Json = 0x06
      +------------------------- tenant:app:agent:run_id


Key Ordering in Storage:
========================

    All keys in sorted order:

    tenant:app:agent:run1 | 0x00 | 0x01 | key_a         <- KV
    tenant:app:agent:run1 | 0x00 | 0x01 | key_b         <- KV
    tenant:app:agent:run1 | 0x00 | 0x02 | 00000001      <- Event
    tenant:app:agent:run1 | 0x00 | 0x02 | 00000002      <- Event
    tenant:app:agent:run1 | 0x00 | 0x03 | state_a       <- State
    tenant:app:agent:run1 | 0x00 | 0x04 | trace_id      <- Trace
    tenant:app:agent:run1 | 0x00 | 0x05 | run_meta      <- Run
    tenant:app:agent:run1 | 0x00 | 0x06 | doc_id_1      <- JSON (NEW)
    tenant:app:agent:run1 | 0x00 | 0x06 | doc_id_2      <- JSON (NEW)
    tenant:app:agent:run2 | 0x00 | 0x01 | key_a         <- Different run


Scan Patterns:
==============

    // All JSON documents in a run
    PREFIX = tenant:app:agent:run1 | 0x00 | 0x06

    // All data in a run (all types)
    PREFIX = tenant:app:agent:run1 | 0x00

    // Specific document
    KEY = tenant:app:agent:run1 | 0x00 | 0x06 | doc_id
```

---

## 11. Array Mutation Semantics

```
+-------------------------------------------------------------------------+
|                    Array Mutation Semantics (M5)                         |
+-------------------------------------------------------------------------+

M5 Restriction: No Structural Array Mutations
=============================================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M5 does NOT provide:                                               │
    │    - array_insert()                                                 │
    │    - array_remove()                                                 │
    │    - array_push()                                                   │
    │    - array_pop()                                                    │
    │    - array_move()                                                   │
    │                                                                     │
    │  This restriction is INTENTIONAL.                                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Why No Structural Array Mutations?
==================================

    Problem: Paths are positional, not identity-based.

    Before:        $.items = [A, B, C]
                   $.items[0] → A
                   $.items[1] → B

    After insert:  $.items = [X, A, B, C]
                   $.items[0] → X  (DIFFERENT!)
                   $.items[1] → A  (DIFFERENT!)

    What should happen to a concurrent read of $.items[1]?
    - Before insert: $.items[1] = B
    - After insert:  $.items[1] = A

    Without stable node identities, this is undefined.


M5 Array Modification Pattern:
==============================

    // Read-modify-write pattern
    let arr = json.get(run_id, doc_id, &path("$.items"))?;
    let mut arr = arr.as_array_mut().unwrap();
    arr.push(new_value);
    json.set(run_id, doc_id, &path("$.items"), arr)?;

    This is:
    - Clear semantics (replace entire array)
    - Conflict detection works ($.items overlaps $.items)
    - No ambiguity about what paths mean


Future Direction (M6+):
=======================

    M6+ may introduce:

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  1. Stable element identities (NodeId)                              │
    │     - Each array element has permanent ID                           │
    │     - Paths can reference by ID, not position                       │
    │                                                                     │
    │  2. Structural array mutations                                      │
    │     - insert(path, index, value)                                    │
    │     - remove(path, index)                                           │
    │                                                                     │
    │  3. Index-safe path semantics                                       │
    │     - $.items[id:abc123] vs $.items[0]                             │
    │     - Position paths vs identity paths                              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    But these features will NOT change the M5 semantic contract.
```

---

## 12. Performance Characteristics

```
+-------------------------------------------------------------------------+
|                   Performance Characteristics (M5)                       |
+-------------------------------------------------------------------------+

M5 Performance Philosophy:
==========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M5 prioritizes CORRECTNESS over SPEED.                             │
    │                                                                     │
    │  JSON operations may be slow. That is acceptable.                   │
    │  Optimization happens AFTER semantics are frozen (M6+).             │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Expected M5 Latencies:
======================

    +-------------------+------------------+----------------------------+
    |    Operation      |  Expected Latency|  Why                       |
    +-------------------+------------------+----------------------------+
    | create()          |     50-100 µs    | Serialize + store          |
    | get(shallow)      |     30-50 µs     | Deserialize + traverse     |
    | get(deep)         |     50-100 µs    | More traversal             |
    | set(any path)     |     100-200 µs   | Deser + modify + ser + sto |
    | patch(n ops)      |     100 + 50n µs | Multiple modifications     |
    +-------------------+------------------+----------------------------+


Non-Regression Requirement:
===========================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  M5 MUST NOT degrade performance of existing primitives.            │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘

    +-------------------+------------------+------------------+
    |    Primitive      |   M4 Target      |  M5 Requirement  |
    +-------------------+------------------+------------------+
    | KVStore get       |      < 5 µs      |      < 5 µs      |
    | KVStore put       |      < 8 µs      |      < 8 µs      |
    | EventLog append   |     < 10 µs      |     < 10 µs      |
    | StateCell cas     |     < 10 µs      |     < 10 µs      |
    +-------------------+------------------+------------------+


How Non-Regression Is Achieved:
===============================

    1. Separate TypeTag (0x06)
       - JSON keys don't collide with other primitives
       - No shared data structures

    2. Separate code paths
       - JSON operations use dedicated functions
       - No hot-path impact on other primitives

    3. Lazy JSON tracking
       - json_read_set/json_write_set only allocated when used
       - Non-JSON transactions have zero overhead

    4. Additive WAL entries
       - New entry types (0x20-0x23)
       - Existing entries unchanged


Why M5 Is Slow (Intentionally):
===============================

    M5 Operation: set($.user.name, "Bob")

    1. Read entire document       ← O(doc size)
    2. Deserialize msgpack        ← O(doc size)
    3. Navigate to $.user.name    ← O(path depth)
    4. Update value               ← O(1)
    5. Serialize to msgpack       ← O(doc size)
    6. Write entire document      ← O(doc size)

    Total: O(doc size) for ANY change

    This is acceptable for M5.


M6+ Optimization Path:
======================

    With structural representation:

    1. Navigate to $.user.name    ← O(path depth)
    2. Update node in place       ← O(1)
    3. Write WAL patch            ← O(patch size)
    4. Mark parents dirty         ← O(path depth)

    Total: O(path depth + patch size)

    10-100× faster for large documents.
```

---

## 13. Layer Dependencies (M5 Updated)

```
+-------------------------------------------------------------------------+
|                      Dependency Graph (M5)                               |
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
    |      +-------+----+------+-----+------+-----+------+----+      |
    |              |           |            |            |           |
    |              |     +-----+-----+      |            |           |
    |              |     | JsonStore |      |            |           |
    |              |     | (M5 NEW)  |      |            |           |
    |              |     +-----+-----+      |            |           |
    |              |           |            |            |           |
    +-------+------+-----------+------------+------------+-----------+
            |                  |
            +------------------+
                    |
          depends on|
                    v
          +--------------------+
          |      Engine        |
          |   (database.rs)    |
          |                    |
          | M5 Extensions:     |
          | - json_read_set    |
          | - json_write_set   |
          | - path_overlaps()  |
          +--------+-----------+
                   |
     +-------------+-------------+
     |             |             |
depends on    depends on    depends on
     |             |             |
     v             v             v
+----------+  +-----------+  +-------------+
| Storage  |  |Durability |  | Concurrency |
| (M4)     |  | (M4+M5)   |  | (M4+M5)     |
|          |  |           |  |             |
|TypeTag:  |  |+JSON WAL  |  |+JSON R/W    |
|Json=0x06 |  | entries   |  | sets        |
+----+-----+  +-----+-----+  +------+------+
     |              |               |
     +--------------+---------------+
                    |
               depends on
                    |
                    v
           +---------------+
           |  Core Types   |
           |  (M1 + M5)    |
           |               |
           | +JsonDocId    |
           | +JsonPath     |
           | +JsonValue    |
           | +JsonPatch    |
           +---------------+


M5 Structural Changes:
======================

    crates/
    ├── core/
    │   └── src/
    │       ├── types.rs          (+JsonDocId)
    │       └── json.rs           (NEW: JsonValue, JsonPath, JsonPatch)
    │
    ├── durability/
    │   └── src/
    │       └── wal.rs            (+JsonWalEntry variants)
    │
    ├── concurrency/
    │   └── src/
    │       └── transaction.rs    (+json_read_set, +json_write_set)
    │
    └── primitives/
        └── src/
            ├── lib.rs            (+json module export)
            └── json.rs           (NEW: JsonStore implementation)


Dependency Rules (Unchanged):
=============================

    Allowed:
    ─────────
    primitives -> engine     (primitives use Database)
    primitives -> core       (primitives use types)
    engine -> storage        (Database uses ShardedStore)
    engine -> durability     (Database uses WAL)
    engine -> concurrency    (Database uses TransactionContext)
    all -> core              (everyone uses core types)

    Forbidden:
    ──────────
    engine -> primitives     (no upward dependencies)
    storage -> engine        (no upward dependencies)
    primitive -> primitive   (no cross-primitive direct deps)

    JsonStore follows the same rules as all other primitives.
```

---

## 14. M5 Philosophy

```
+-------------------------------------------------------------------------+
|                           M5 Philosophy                                  |
+-------------------------------------------------------------------------+

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │     M5 does not aim to be fast.                                     │
    │                                                                     │
    │     M5 aims to LOCK IN SEMANTICS.                                   │
    │                                                                     │
    │     M5 freezes the semantic model.                                  │
    │     M6+ optimizes the implementation.                               │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M5 Locks In:
=================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ✓ Path-level mutation semantics                                    │
    │    - get/set/delete operate on paths                                │
    │    - Paths are positional, not identity-based                       │
    │                                                                     │
    │  ✓ Region-based conflict detection                                  │
    │    - Overlap = conflict                                             │
    │    - Siblings don't conflict                                        │
    │                                                                     │
    │  ✓ Patch-based WAL format                                           │
    │    - Never log full documents                                       │
    │    - Idempotent but ordered                                         │
    │                                                                     │
    │  ✓ Weak snapshot isolation                                          │
    │    - Explicit failure on concurrent modification                    │
    │    - No historical reads (yet)                                      │
    │                                                                     │
    │  ✓ Transaction integration                                          │
    │    - Same atomicity as other primitives                             │
    │    - Cross-primitive transactions work                              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M5 Defers:
===============

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  → M6: Structural storage representation                            │
    │        Per-node versioning                                          │
    │        Structural sharing                                           │
    │        Full snapshot isolation (MVCC)                               │
    │        Array insert/remove with stable IDs                          │
    │                                                                     │
    │  → M7: Diff operations                                              │
    │        Time travel queries                                          │
    │                                                                     │
    │  → M11: Query language                                              │
    │         Indexes                                                     │
    │         Projections                                                 │
    │         Aggregations                                                │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


Why Semantics Before Optimization:
==================================

    Once you ship an API, you're stuck with it.

    +-------------------------------------------------------------------+
    |                                                                    |
    |  Semantic decisions are PERMANENT:                                |
    |  - What does $.items[0] mean?                                     |
    |  - When do operations conflict?                                   |
    |  - What does the WAL contain?                                     |
    |                                                                    |
    |  Implementation decisions are CHANGEABLE:                         |
    |  - How is JSON stored internally?                                 |
    |  - How fast are operations?                                       |
    |  - How is memory laid out?                                        |
    |                                                                    |
    +-------------------------------------------------------------------+

    M5 freezes the permanent decisions.
    M6+ can change the changeable decisions freely.


The Simple Blob Implementation:
===============================

    M5's blob-based storage is:

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  ✓ CORRECT                                                          │
    │    - All semantic invariants hold                                   │
    │    - All tests pass                                                 │
    │    - All edge cases handled                                         │
    │                                                                     │
    │  ✓ SIMPLE                                                           │
    │    - Easy to reason about                                           │
    │    - Easy to debug                                                  │
    │    - Easy to test                                                   │
    │                                                                     │
    │  ✓ REPLACEABLE                                                      │
    │    - Structural representation can replace blobs                    │
    │    - Same API, different internals                                  │
    │    - No breaking changes                                            │
    │                                                                     │
    │  ✗ FAST                                                             │
    │    - O(doc size) for any change                                     │
    │    - Acceptable for M5                                              │
    │    - Fixed in M6+                                                   │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘
```

---

These diagrams illustrate the key architectural components and flows for M5's JSON Primitive milestone. M5 builds upon M4's performance optimizations while adding a sixth primitive with path-level mutation semantics.

**Key Design Points Reflected in These Diagrams**:
- JSON is a first-class primitive with its own TypeTag (0x06)
- Paths are positional (not identity-based) - a critical semantic decision
- Conflict detection is region-based (path overlap = conflict)
- WAL entries are patch-based (never full documents)
- Weak snapshot isolation (explicit failure on concurrent modification)
- Lazy JSON tracking (zero overhead for non-JSON transactions)
- Document-level versioning (simpler, more false conflicts, correctible in M6+)
- No structural array mutations (ambiguous semantics without stable IDs)

**M5 Philosophy**: M5 does not aim to be fast. M5 aims to *lock in semantics*. M5 freezes the semantic model. M6+ optimizes the implementation.
