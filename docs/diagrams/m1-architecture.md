# M1 Architecture Diagrams

This document contains visual representations of the M1 architecture.

---

## 1. System Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Application Layer                           │
│                    (Agent Applications using DB)                     │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 │ API calls
                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         Primitives Layer                            │
│                         (Stateless Facades)                         │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐            │
│  │   KVStore    │  │  EventLog*   │  │    Trace*    │            │
│  │              │  │              │  │              │            │
│  │ - get()      │  │ - append()   │  │ - record()   │            │
│  │ - put()      │  │ - read()     │  │ - query()    │            │
│  │ - delete()   │  │ - scan()     │  │              │            │
│  │ - list()     │  │              │  │              │            │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘            │
│         │                 │                  │                     │
│         └─────────────────┼──────────────────┘                     │
│                           │                                        │
│                    All delegate to Database                        │
│                                                                     │
│  * EventLog and Trace deferred to M3                               │
└────────────────────────────┬────────────────────────────────────────┘
                             │
                             │ Database API
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          Engine Layer                               │
│                   (Orchestration & Coordination)                    │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────┐    │
│  │                     Database                              │    │
│  │                                                           │    │
│  │  Responsibilities:                                        │    │
│  │  • Run Lifecycle (begin_run, end_run)                    │    │
│  │  • Operation Coordination (put, get, delete)             │    │
│  │  • Atomic Updates (Storage + WAL)                        │    │
│  │  • Recovery on Startup                                   │    │
│  │                                                           │    │
│  │  State:                                                   │    │
│  │  • storage: Arc<UnifiedStore>                            │    │
│  │  • wal: Arc<Mutex<WAL>>                                  │    │
│  │  • run_tracker: Arc<RunTracker>                          │    │
│  │  • next_txn_id: AtomicU64                                │    │
│  └───────────────────────────────────────────────────────────┘    │
│                             │                                       │
│                             │ Calls both layers                     │
│                             ▼                                       │
│              ┌──────────────┴──────────────┐                       │
│              │                             │                       │
└──────────────┼─────────────────────────────┼───────────────────────┘
               │                             │
               ▼                             ▼
┌──────────────────────────┐    ┌────────────────────────────────────┐
│    Storage Layer         │    │      Durability Layer              │
│                          │    │                                    │
│  ┌────────────────────┐ │    │  ┌──────────────────────────────┐ │
│  │  UnifiedStore      │ │    │  │        WAL                   │ │
│  │                    │ │    │  │                              │ │
│  │  Data:             │ │    │  │  • Append-only log           │ │
│  │  • BTreeMap        │ │    │  │  • Durability modes          │ │
│  │  • RwLock          │ │    │  │    - Strict (fsync always)   │ │
│  │  • AtomicU64       │ │    │  │    - Batched (100ms/1000ops) │ │
│  │                    │ │    │  │    - Async (background)      │ │
│  │  Indices:          │ │    │  │  • CRC32 checksums           │ │
│  │  • run_index       │ │    │  └──────────────────────────────┘ │
│  │  • type_index      │ │    │                                    │
│  │  • ttl_index       │ │    │  ┌──────────────────────────────┐ │
│  │                    │ │    │  │      Recovery                │ │
│  │  Operations:       │ │    │  │                              │ │
│  │  • get()           │ │    │  │  • WAL replay                │ │
│  │  • put()           │ │    │  │  • Transaction validation    │ │
│  │  • delete()        │ │    │  │  • Incomplete txn handling   │ │
│  │  • scan_by_run()   │ │    │  └──────────────────────────────┘ │
│  │  • scan_prefix()   │ │    │                                    │
│  └────────────────────┘ │    │  ┌──────────────────────────────┐ │
│                          │    │  │      Encoding                │ │
│  Implements:             │    │  │                              │ │
│  Storage trait           │    │  │  • bincode serialization     │ │
│                          │    │  │  • CRC32 calculation         │ │
└──────────────────────────┘    │  │  • Entry format:             │ │
                                │  │    [len][type][payload][crc] │ │
                                │  └──────────────────────────────┘ │
                                └────────────────────────────────────┘
                                            │
                                            │ Uses
                                            ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          Core Types Layer                           │
│                      (Foundation Definitions)                       │
│                                                                     │
│  Types:                                                             │
│  • RunId          - Unique run identifier (UUID)                    │
│  • Namespace      - Hierarchical scope (tenant/app/agent/run)       │
│  • Key            - Composite key (namespace + type_tag + user_key) │
│  • TypeTag        - Type discriminator (KV, Event, Trace, etc.)     │
│  • Value          - Unified value enum                              │
│  • VersionedValue - Value with version/timestamp/TTL                │
│                                                                     │
│  Traits:                                                            │
│  • Storage        - Storage abstraction (enables replacement)       │
│  • SnapshotView   - Snapshot abstraction (prevents ossification)    │
│                                                                     │
│  Errors:                                                            │
│  • Error          - Top-level error enum                            │
│  • StorageError   - Storage-specific errors                         │
│  • DurabilityError- WAL/recovery errors                             │
│  • PrimitiveError - Primitive-specific errors                       │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 2. Data Flow: Write Operation

```
┌──────────┐
│   App    │
└────┬─────┘
     │ kv.put(run_id, "key", b"value")
     ▼
┌─────────────────┐
│    KVStore      │  Stateless Facade
│                 │
│ pub fn put(...) │
└────┬────────────┘
     │ db.put(run_id, key, Value::Bytes(value))
     ▼
┌──────────────────────────────────────────────────────────┐
│                     Database                             │
│                                                          │
│  1. Allocate txn_id (AtomicU64::fetch_add)              │
│  2. Create Key (namespace + TypeTag::KV + user_key)     │
│  3. Acquire WAL lock (Mutex::lock)                      │
│                                                          │
│     ┌─────────────────────────────────────┐            │
│     │  Atomic Block (WAL lock held)       │            │
│     │                                      │            │
│     │  a) WAL.append(BeginTxn)            │            │
│     │  b) Storage.put(key, value) ────┐   │            │
│     │  c) WAL.append(Write)            │   │            │
│     │  d) WAL.append(CommitTxn)        │   │            │
│     │                                  │   │            │
│     └──────────────────────────────────┼───┘            │
│                                        │                │
│  4. Release WAL lock                   │                │
│  5. Return version                     │                │
└────────────────────────────────────────┼────────────────┘
                                         │
                    ┌────────────────────┴─────────────────────┐
                    │                                          │
                    ▼                                          ▼
         ┌────────────────────┐                    ┌────────────────────┐
         │  UnifiedStore      │                    │       WAL          │
         │                    │                    │                    │
         │  1. Alloc version  │                    │  File entries:     │
         │     (AtomicU64)    │                    │                    │
         │  2. Create         │                    │  [BeginTxn]        │
         │     VersionedValue │                    │    txn_id: 42      │
         │  3. Lock data      │                    │    run_id: ...     │
         │  4. Insert into    │                    │                    │
         │     BTreeMap       │                    │  [Write]           │
         │  5. Update indices │                    │    key: ...        │
         │     - run_index    │                    │    value: ...      │
         │     - type_index   │                    │    version: 100    │
         │     - ttl_index    │                    │                    │
         │  6. Unlock         │                    │  [CommitTxn]       │
         │  7. Return version │                    │    txn_id: 42      │
         │                    │                    │                    │
         └────────────────────┘                    │  [CRC32]           │
                                                   │                    │
                                                   │  Durability mode   │
                                                   │  triggers fsync    │
                                                   └────────────────────┘
```

---

## 3. Data Flow: Read Operation

```
┌──────────┐
│   App    │
└────┬─────┘
     │ kv.get(run_id, "key")
     ▼
┌─────────────────┐
│    KVStore      │  Stateless Facade
│                 │
│ pub fn get(...) │
└────┬────────────┘
     │ db.get(run_id, key)
     ▼
┌──────────────────────────────────────────┐
│            Database                      │
│                                          │
│  1. Create Key (namespace + type + key) │
│  2. Call storage.get(key)               │
│  3. Extract Value::Bytes                │
│  4. Return bytes                        │
└────┬─────────────────────────────────────┘
     │ storage.get(&key)
     ▼
┌─────────────────────────────────────┐
│         UnifiedStore                │
│                                     │
│  1. Acquire read lock (RwLock)     │
│  2. Lookup in BTreeMap             │
│  3. Check if expired (TTL)         │
│  4. Return VersionedValue or None  │
│  5. Release lock                   │
└─────────────────────────────────────┘

Note: Read operations do NOT touch WAL
      (only writes are logged for durability)
```

---

## 4. Recovery Flow

```
┌─────────────────────────────────────────────────────────────┐
│                    Database::open(path)                     │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
            ┌────────────────────────┐
            │  1. Open WAL File      │
            │     (or create new)    │
            └────────┬───────────────┘
                     │
                     ▼
            ┌────────────────────────┐
            │  2. Create Empty       │
            │     UnifiedStore       │
            └────────┬───────────────┘
                     │
                     ▼
    ┌────────────────────────────────────────────────────┐
    │          3. Recovery: replay_wal()                 │
    │                                                    │
    │  ┌──────────────────────────────────────────┐    │
    │  │  a) Read all WAL entries                 │    │
    │  │     - Decode [len][type][payload][crc]   │    │
    │  │     - Verify CRC32 checksums             │    │
    │  │     - Stop at corruption/EOF             │    │
    │  └───────────────┬──────────────────────────┘    │
    │                  │                                │
    │  ┌───────────────▼──────────────────────────┐    │
    │  │  b) Validate Transactions                │    │
    │  │     - Check BeginTxn → Write* → Commit   │    │
    │  │     - Find incomplete txns               │    │
    │  │     - Warn about orphaned entries        │    │
    │  └───────────────┬──────────────────────────┘    │
    │                  │                                │
    │  ┌───────────────▼──────────────────────────┐    │
    │  │  c) Group Entries by txn_id              │    │
    │  │                                           │    │
    │  │     Transaction {                        │    │
    │  │       txn_id: 42,                        │    │
    │  │       run_id: ...,                       │    │
    │  │       entries: [BeginTxn, Write, ...],   │    │
    │  │       committed: bool                    │    │
    │  │     }                                     │    │
    │  └───────────────┬──────────────────────────┘    │
    │                  │                                │
    │  ┌───────────────▼──────────────────────────┐    │
    │  │  d) Apply Committed Transactions Only    │    │
    │  │                                           │    │
    │  │     For each committed txn:              │    │
    │  │       For each Write entry:              │    │
    │  │         storage.put(key, value)          │    │
    │  │       For each Delete entry:             │    │
    │  │         storage.delete(key)              │    │
    │  │                                           │    │
    │  │     Discard incomplete transactions      │    │
    │  └───────────────┬──────────────────────────┘    │
    │                  │                                │
    │  ┌───────────────▼──────────────────────────┐    │
    │  │  e) Log Recovery Stats                   │    │
    │  │     - Txns applied                       │    │
    │  │     - Writes applied                     │    │
    │  │     - Deletes applied                    │    │
    │  │     - Incomplete txns discarded          │    │
    │  └──────────────────────────────────────────┘    │
    └────────────────────────────────────────────────────┘
                         │
                         ▼
            ┌────────────────────────┐
            │  4. Return Database    │
            │     (ready for use)    │
            └────────────────────────┘
```

---

## 5. Key Structure and Ordering

```
┌─────────────────────────────────────────────────────────────┐
│                       Key Structure                         │
└─────────────────────────────────────────────────────────────┘

Key = Namespace + TypeTag + user_key

┌──────────────────────────────────────────────────────────────┐
│  Namespace (4 components)                                    │
├──────────────────────────────────────────────────────────────┤
│  • tenant:   "acme"                                          │
│  • app:      "chatbot"                                       │
│  • agent:    "agent-42"                                      │
│  • run_id:   UUID(123e4567-e89b-12d3-a456-426614174000)     │
└──────────────────────────────────────────────────────────────┘
                         +
┌──────────────────────────────────────────────────────────────┐
│  TypeTag (1 byte enum)                                       │
├──────────────────────────────────────────────────────────────┤
│  • KV = 0                                                    │
│  • Event = 1                                                 │
│  • StateMachine = 2                                          │
│  • Trace = 3                                                 │
│  • RunMetadata = 4                                           │
└──────────────────────────────────────────────────────────────┘
                         +
┌──────────────────────────────────────────────────────────────┐
│  user_key (variable bytes)                                   │
├──────────────────────────────────────────────────────────────┤
│  • "session_state" (string)                                  │
│  • [0x00, 0x00, 0x00, 0x01] (binary, e.g., sequence number)  │
│  • Any Vec<u8>                                               │
└──────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                   BTreeMap Ordering                         │
├─────────────────────────────────────────────────────────────┤
│  Keys sorted by:                                            │
│    1. namespace.tenant                                      │
│    2. namespace.app                                         │
│    3. namespace.agent                                       │
│    4. namespace.run_id                                      │
│    5. type_tag                                              │
│    6. user_key                                              │
│                                                             │
│  Example sorted keys:                                       │
│    ("acme", "app1", "agent1", run1, KV, "a")               │
│    ("acme", "app1", "agent1", run1, KV, "b")               │
│    ("acme", "app1", "agent1", run1, Event, [0x01])         │
│    ("acme", "app1", "agent1", run2, KV, "a")               │
│    ("acme", "app2", "agent1", run1, KV, "a")               │
│    ("corp", "app1", "agent1", run1, KV, "a")               │
│                                                             │
│  Benefits:                                                  │
│    • Efficient range scans by namespace                     │
│    • Efficient scans by run_id (same namespace)             │
│    • Efficient scans by type_tag                            │
└─────────────────────────────────────────────────────────────┘
```

---

## 6. Concurrency Model (M1)

```
┌─────────────────────────────────────────────────────────────┐
│                Thread Safety Architecture                   │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                      Database                               │
│                                                             │
│  storage: Arc<UnifiedStore>     ← Shared across threads    │
│  wal: Arc<Mutex<WAL>>            ← Mutex: one writer       │
│  run_tracker: Arc<RunTracker>   ← RwLock internally        │
└─────────────────────────────────────────────────────────────┘
           │                              │
           ▼                              ▼
┌────────────────────────┐    ┌───────────────────────────────┐
│    UnifiedStore        │    │         WAL                   │
│                        │    │                               │
│  data: RwLock<BTree>   │    │  Mutex<File>                  │
│                        │    │                               │
│  Many readers OR       │    │  Only one writer at a time    │
│  one writer            │    │                               │
└────────────────────────┘    └───────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                   Concurrency Scenario                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Thread 1 (Write):                                          │
│    1. Lock WAL (Mutex::lock)           ← Blocks others      │
│    2. Append BeginTxn                                       │
│    3. Lock storage.data (RwLock write) ← Blocks all         │
│    4. Insert into BTreeMap                                  │
│    5. Unlock storage.data                                   │
│    6. Append Write to WAL                                   │
│    7. Append CommitTxn                                      │
│    8. Unlock WAL                                            │
│                                                             │
│  Thread 2, 3, 4 (Read):                                     │
│    1. Lock storage.data (RwLock read)  ← Concurrent OK      │
│    2. Read from BTreeMap                                    │
│    3. Unlock storage.data                                   │
│                                                             │
│  Thread 5 (Write):                                          │
│    1. Wait for Thread 1 to release WAL ← Serialized         │
│    2. Lock WAL                                              │
│    3. ... (same as Thread 1)                                │
│                                                             │
│  Known Bottlenecks:                                         │
│    • WAL Mutex: Serializes all writes                       │
│    • data RwLock: Writers block readers                     │
│    • global_version AtomicU64: Contention on fetch_add      │
│                                                             │
│  Acceptable for M1: Agents don't write concurrently much    │
└─────────────────────────────────────────────────────────────┘
```

---

## 7. WAL Entry Format

```
┌─────────────────────────────────────────────────────────────┐
│                  WAL Entry On-Disk Format                   │
└─────────────────────────────────────────────────────────────┘

Single Entry:
┌────────┬──────┬─────────────────┬─────────┐
│ Length │ Type │    Payload      │  CRC32  │
│ 4 bytes│1 byte│   N bytes       │ 4 bytes │
└────────┴──────┴─────────────────┴─────────┘

Length:  u32, little-endian, total size of (type + payload + crc)
Type:    u8, entry type tag (BeginTxn=1, Write=2, Delete=3, ...)
Payload: bincode-serialized WALEntry struct
CRC32:   Checksum over (type + payload)

┌─────────────────────────────────────────────────────────────┐
│                    Example WAL File                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Offset  Entry                                              │
│  ------  -----                                              │
│  0       [BeginTxn]                                         │
│           ├─ length: 120                                    │
│           ├─ type: 1                                        │
│           ├─ payload: {txn_id: 42, run_id: ..., ts: ...}   │
│           └─ crc: 0xABCD1234                                │
│                                                             │
│  124     [Write]                                            │
│           ├─ length: 256                                    │
│           ├─ type: 2                                        │
│           ├─ payload: {run_id, key, value, version}         │
│           └─ crc: 0x12345678                                │
│                                                             │
│  384     [CommitTxn]                                        │
│           ├─ length: 64                                     │
│           ├─ type: 4                                        │
│           ├─ payload: {txn_id: 42, run_id: ...}            │
│           └─ crc: 0x9ABCDEF0                                │
│                                                             │
│  448     [BeginTxn]                                         │
│           ├─ ...                                            │
│           └─ ...                                            │
│                                                             │
│  ...     (more entries)                                     │
│                                                             │
│  EOF                                                        │
└─────────────────────────────────────────────────────────────┘

CRC Verification on Read:
┌────────────────────────────────────────────────────┐
│  1. Read [length]                                  │
│  2. Read [type][payload][crc] (length bytes)       │
│  3. Compute CRC32(type + payload)                  │
│  4. Compare with stored CRC                        │
│     ├─ Match: Decode payload, return entry         │
│     └─ Mismatch: Return CorruptionError            │
└────────────────────────────────────────────────────┘
```

---

## 8. Transaction Lifecycle

```
┌─────────────────────────────────────────────────────────────┐
│                  Transaction States (M1)                    │
└─────────────────────────────────────────────────────────────┘

M1: Implicit Single-Operation Transactions

┌──────────┐
│   App    │
└────┬─────┘
     │ db.put(run_id, "key", value)
     ▼
┌─────────────────────────────────────────────────────────────┐
│                    Database.put()                           │
│                                                             │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Transaction Lifecycle                                │  │
│  │                                                      │  │
│  │  1. ALLOCATE txn_id                                 │  │
│  │     next_txn_id.fetch_add(1) → 42                   │  │
│  │                                                      │  │
│  │  2. BEGIN (write to WAL)                            │  │
│  │     WALEntry::BeginTxn {                            │  │
│  │       txn_id: 42,                                   │  │
│  │       run_id: ...,                                  │  │
│  │       timestamp: now()                              │  │
│  │     }                                                │  │
│  │                                                      │  │
│  │  3. EXECUTE (write to storage)                      │  │
│  │     storage.put(key, value) → version 100           │  │
│  │                                                      │  │
│  │  4. LOG OPERATION (write to WAL)                    │  │
│  │     WALEntry::Write {                               │  │
│  │       run_id: ...,                                  │  │
│  │       key: ...,                                     │  │
│  │       value: ...,                                   │  │
│  │       version: 100                                  │  │
│  │     }                                                │  │
│  │                                                      │  │
│  │  5. COMMIT (write to WAL)                           │  │
│  │     WALEntry::CommitTxn {                           │  │
│  │       txn_id: 42,                                   │  │
│  │       run_id: ...                                   │  │
│  │     }                                                │  │
│  │                                                      │  │
│  │  6. RETURN version                                  │  │
│  │     → 100                                            │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘

On Crash:
┌─────────────────────────────────────────────────────────────┐
│  Recovery examines transaction completeness:                │
│                                                             │
│  Complete (has CommitTxn):                                  │
│    BeginTxn(42) → Write → CommitTxn(42) ✓ Applied          │
│                                                             │
│  Incomplete (no CommitTxn):                                 │
│    BeginTxn(43) → Write → [crash] ✗ Discarded              │
│                                                             │
│  Aborted (has AbortTxn):                                    │
│    BeginTxn(44) → Write → AbortTxn(44) ✗ Discarded         │
└─────────────────────────────────────────────────────────────┘

M2 Future: Multi-Operation Transactions
┌─────────────────────────────────────────────────────────────┐
│  txn = db.begin_transaction(run_id)                         │
│  txn.put("key1", value1)      ← Buffered                    │
│  txn.put("key2", value2)      ← Buffered                    │
│  txn.delete("key3")           ← Buffered                    │
│  txn.commit()                 ← Atomic (all or nothing)     │
└─────────────────────────────────────────────────────────────┘
```

---

## 9. Layer Dependencies

```
┌─────────────────────────────────────────────────────────────┐
│                  Dependency Graph                           │
└─────────────────────────────────────────────────────────────┘

                        ┌────────────┐
                        │    App     │
                        └──────┬─────┘
                               │
                               ▼
                      ┌─────────────────┐
                      │   Primitives    │
                      │   (kv.rs)       │
                      └────────┬────────┘
                               │ depends on
                               ▼
                      ┌─────────────────┐
                      │     Engine      │
                      │  (database.rs)  │
                      │     (run.rs)    │
                      └────┬────────┬───┘
                           │        │
              depends on   │        │  depends on
                           │        │
             ┌─────────────▼──┐  ┌──▼─────────────┐
             │   Storage      │  │   Durability   │
             │  (unified.rs)  │  │    (wal.rs)    │
             │   (index.rs)   │  │  (recovery.rs) │
             └────────┬───────┘  └────┬───────────┘
                      │               │
         depends on   │               │  depends on
                      │               │
                      └───────┬───────┘
                              │
                              ▼
                      ┌───────────────┐
                      │  Core Types   │
                      │  (types.rs)   │
                      │  (value.rs)   │
                      │  (error.rs)   │
                      │  (traits.rs)  │
                      └───────────────┘

Rules:
✓ Allowed:    Child → Parent (downward dependencies)
✗ Forbidden:  Parent → Child (upward dependencies)
✗ Forbidden:  Sibling → Sibling (peer dependencies at same level)

Examples:
✓ primitives/kv.rs     → engine/database.rs    (OK)
✓ engine/database.rs   → storage/unified.rs    (OK)
✓ engine/database.rs   → durability/wal.rs     (OK)
✓ storage/unified.rs   → core/types.rs         (OK)
✗ storage/unified.rs   → engine/database.rs    (FORBIDDEN)
✗ primitives/kv.rs     → primitives/event.rs   (FORBIDDEN)
✗ storage/unified.rs   → durability/wal.rs     (FORBIDDEN)
```

---

## 10. File System Layout

```
┌─────────────────────────────────────────────────────────────┐
│              Database Directory Structure                   │
└─────────────────────────────────────────────────────────────┘

<data_dir>/
├── wal/
│   └── current.wal              ← Append-only transaction log
│
└── (future: snapshots/)
    ├── snapshot_<uuid>.dat      ← Full storage snapshot (M4)
    └── snapshot_<uuid>.meta     ← Snapshot metadata (M4)

Example:
/var/lib/agent-db/
├── wal/
│   └── current.wal              (523 MB)
└── (no snapshots yet in M1)

WAL File Growth (M1):
- Grows unbounded (no rotation in M1)
- Typical size: 1-10 MB for short-lived agents
- Large deployments: 100MB-1GB before restart
- M4 will add snapshots + WAL truncation

File Permissions:
- data_dir:     rwx------ (0700)
- wal/:         rwx------ (0700)
- current.wal:  rw------- (0600)
```

---

These diagrams provide visual representations of all major architectural aspects of M1. They can be used for:
- Onboarding new developers
- Design reviews
- Documentation
- Debugging and troubleshooting
- Architecture discussions
