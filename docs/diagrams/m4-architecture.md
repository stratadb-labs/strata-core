# M4 Architecture Diagrams: Performance

This document contains visual representations of the M4 architecture focused on performance optimizations and durability modes.

**Architecture Spec Version**: 1.1 (Validated)

---

## Critical Invariants (Reference)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         M4 IMPLEMENTATION INVARIANTS                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  1. ATOMICITY SCOPE                                                         │
│     Transactions are atomic within a single RunId.                          │
│     Cross-run writes are NOT guaranteed atomic.                             │
│                                                                             │
│  2. SNAPSHOT SEMANTIC INVARIANT                                             │
│     Fast-path reads must be observationally equivalent to a                 │
│     snapshot-based transaction.                                             │
│     • No dirty reads                                                        │
│     • No torn reads                                                         │
│     • No stale reads                                                        │
│     • No mixing versions across keys                                        │
│                                                                             │
│  3. REQUIRED DEPENDENCIES                                                   │
│     dashmap = "5"                                                           │
│     rustc-hash = "1.1"  (NOT fxhash)                                       │
│     parking_lot = "0.12"                                                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 1. System Architecture Overview (M4)

```
+-------------------------------------------------------------------------+
|                           Application Layer                              |
|                      (Agent Applications using DB)                       |
+-----------------------------------+-------------------------------------+
                                    |
                                    | High-level typed APIs
                                    v
+-------------------------------------------------------------------------+
|                          Primitives Layer (M3)                           |
|                          (Stateless Facades)                             |
|                                                                          |
|  +-------------+  +-------------+  +--------------+  +-------------+    |
|  |  KV Store   |  |  Event Log  |  |  StateCell   |  |Trace Store  |    |
|  |             |  |             |  |              |  |             |    |
|  | - get() ◄───────────────────────────────────────────────────────────┐|
|  | - put()     |  | - append()  |  | - read()     |  | - record()  |   ||
|  | - delete()  |  | - read()    |  | - init()     |  | - get()     |   ||
|  | - list()    |  | - iter()    |  | - cas()      |  | - query_*() |   ||
|  +------+------+  +------+------+  +------+-------+  +------+------+   ||
|         |                |                |                |           ||
|         +----------------+----------------+----------------+           ||
|                                   |                                    ||
|  +-----------------------------------------------------------------+  ||
|  |                         Run Index                                 |  ||
|  +-----------------------------------------------------------------+  ||
|                               |                                        ||
+-------------------------------+----------------------------------------+|
                                |                                         |
                                | Database transaction API                |
                                v                                         |
+-------------------------------------------------------------------------+
|                         Engine Layer (M1-M2-M4)                          |
|                   (Orchestration & Coordination)                         |
|                                                                          |
|  +-------------------------------------------------------------------+  |
|  |                          Database                                  |  |
|  |                                                                    |  |
|  |  M4 NEW: Durability Mode Selection                                |  |
|  |  - InMemory    (no WAL, no fsync)  ─────── <3µs latency           |  |
|  |  - Buffered    (async fsync)       ─────── <30µs latency          |  |
|  |  - Strict      (sync fsync)        ─────── ~2ms latency           |  |
|  |                                                                    |  |
|  |  M4 NEW: Transaction Pooling                                      |  |
|  |  - Thread-local pools for zero-allocation hot path                |  |
|  |                                                                    |  |
|  |  M4 NEW: Read Path Optimization ─────────────────────────────────►┘  |
|  |  - Direct snapshot reads bypass full transaction                  |  |
|  +-------------------------------------------------------------------+  |
|                               |                                          |
+----------+-------------------+-------------------+-----------------------+
           |                   |                   |
           v                   v                   v
+------------------+  +-------------------+  +------------------------+
|  Storage (M4)    |  | Durability (M4)   |  | Concurrency (M4)       |
|     UPDATED      |  |     UPDATED       |  |      UPDATED           |
|                  |  |                   |  |                        |
| - ShardedStore   |  | - InMemoryMode    |  | - Transaction Pooling  |
| - DashMap        |  | - BufferedMode    |  | - Read Fast Path       |
| - FxHashMap      |  | - StrictMode      |  | - Lock Sharding        |
| - Per-RunId      |  | - Background flush|  | - Per-RunId Shards     |
+------------------+  +-------------------+  +------------------------+
           |                   |                   |
           +-------------------+-------------------+
                               |
                               v
+-------------------------------------------------------------------------+
|                         Core Types Layer (M1)                            |
|                       (Foundation Definitions)                           |
+-------------------------------------------------------------------------+
```

---

## 2. Durability Modes

```
+-------------------------------------------------------------------------+
|                      Durability Mode Selection (M4)                      |
+-------------------------------------------------------------------------+

                           User Configures Mode
                                    |
                                    v
                    +-------------------------------+
                    |    Database::builder()        |
                    |    .durability(mode)          |
                    |    .open()                    |
                    +-------------------------------+
                                    |
            +-----------------------+-----------------------+
            |                       |                       |
            v                       v                       v
    +---------------+       +---------------+       +---------------+
    |   InMemory    |       |   Buffered    |       |    Strict     |
    +---------------+       +---------------+       +---------------+
    | WAL:    None  |       | WAL:  Append  |       | WAL:  Append  |
    | fsync:  None  |       | fsync: Async  |       | fsync:  Sync  |
    | Latency: <3µs |       | Latency: <30µs|       | Latency: ~2ms |
    | Loss:   All   |       | Loss: Bounded |       | Loss:   Zero  |
    +---------------+       +---------------+       +---------------+
            |                       |                       |
            v                       v                       v
    +---------------+       +---------------+       +---------------+
    | commit() {    |       | commit() {    |       | commit() {    |
    |   // No WAL   |       |   wal.append()|       |   wal.append()|
    |   storage     |       |   storage     |       |   wal.fsync() |
    |     .apply()  |       |     .apply()  |       |   storage     |
    | }             |       |   if should   |       |     .apply()  |
    |               |       |     _flush()  |       | }             |
    |               |       |     flush_    |       |               |
    |               |       |       async() |       |               |
    +---------------+       +---------------+       +---------------+


ACID Properties by Mode:
========================

    +----------------+------------+------------+------------+
    |   Property     |  InMemory  |  Buffered  |   Strict   |
    +----------------+------------+------------+------------+
    | Atomicity      |     ✓      |     ✓      |     ✓      |
    | Consistency    |     ✓      |     ✓      |     ✓      |
    | Isolation      |     ✓      |     ✓      |     ✓      |
    | Durability     |     ✗      |  Bounded   |     ✓      |
    +----------------+------------+------------+------------+

    All modes provide the SAME ACI guarantees.
    Only D (Durability) differs by mode.
```

---

## 3. Sharded Storage Architecture (M4)

```
+-------------------------------------------------------------------------+
|                    Sharded Storage Architecture (M4)                     |
+-------------------------------------------------------------------------+

M3 Architecture (BOTTLENECK):
=============================

    +-------------------------------------------+
    |              UnifiedStore                  |
    |                                            |
    |    +----------------------------------+   |
    |    |    RwLock<BTreeMap>              |   |
    |    |    (GLOBAL LOCK)                 |   |
    |    |                                  |   |
    |    |  ┌─────────────────────────────┐ |   |
    |    |  │         BTreeMap            │ |   |    Problems:
    |    |  │                             │ |   |    - Single global lock
    |    |  │   All runs                  │ |   |    - O(log n) lookups
    |    |  │   All keys                  │ |   |    - Pointer-heavy tree
    |    |  │   All contention            │ |   |    - Cache unfriendly
    |    |  │                             │ |   |
    |    |  └─────────────────────────────┘ |   |
    |    +----------------------------------+   |
    +-------------------------------------------+


M4 Architecture (SHARDED):
==========================

    +-----------------------------------------------------------+
    |                      ShardedStore                          |
    |                                                            |
    |   +---------------------------------------------------+   |
    |   |         DashMap<RunId, Shard, FxBuildHasher>       |   |
    |   |              (Lock-free reads, sharded writes)     |   |
    |   |                                                    |   |
    |   |  ┌─────────┐  ┌─────────┐  ┌─────────┐           |   |
    |   |  │ RunId A │  │ RunId B │  │ RunId C │    ...    |   |
    |   |  │         │  │         │  │         │           |   |
    |   |  │ Shard { │  │ Shard { │  │ Shard { │           |   |
    |   |  │  FxHash │  │  FxHash │  │  FxHash │           |   |
    |   |  │  Map    │  │  Map    │  │  Map    │           |   |
    |   |  │ }       │  │ }       │  │ }       │           |   |
    |   |  └────┬────┘  └────┬────┘  └────┬────┘           |   |
    |   |       │            │            │                |   |
    |   +-------│------------│------------│----------------+   |
    |           │            │            │                    |
    +-----------|------------|------------|--------------------+
                |            |            |
                v            v            v
        +-------------+ +-------------+ +-------------+
        | Agent A's   | | Agent B's   | | Agent C's   |
        | data        | | data        | | data        |
        | (isolated)  | | (isolated)  | (isolated)   |
        +-------------+ +-------------+ +-------------+


Why DashMap + HashMap (not RwLock + BTreeMap):
==============================================

    +------------------+-------------------+-------------------+
    |     Aspect       | RwLock + BTreeMap | DashMap + HashMap |
    +------------------+-------------------+-------------------+
    | Read locking     | Global RwLock     | Lock-free reads   |
    | Write locking    | Global RwLock     | Per-shard (16-way)|
    | Lookup           | O(log n) + chase  | O(1) + probe      |
    | Cache behavior   | Poor (tree)       | Good (bucket)     |
    | Ordered iter     | Native            | Requires sort     |
    +------------------+-------------------+-------------------+

    Trade-off: list() operations now require sort.
    Acceptable: list() is NOT on hot path. get/put are 10-100× more frequent.


WARNING - Still Provisional:
============================

    DashMap + HashMap is a TACTICAL improvement, not final architecture.

    Still has:
    - Internal locks (DashMap)
    - Allocation on growth (HashMap)
    - Clone on read (values)

    These require M5+ work for true Redis parity.
```

---

## 4. Lock Sharding by RunId

```
+-------------------------------------------------------------------------+
|                       Lock Sharding Design (M4)                          |
+-------------------------------------------------------------------------+

M3: All operations contend on single lock
=========================================

    Thread 1 (RunId A)    Thread 2 (RunId B)    Thread 3 (RunId C)
          │                      │                      │
          │                      │                      │
          ▼                      ▼                      ▼
    ┌─────────────────────────────────────────────────────────┐
    │                    GLOBAL RwLock                         │
    │                   (serialization point)                  │
    └─────────────────────────────────────────────────────────┘
                               │
                               ▼
                    ┌─────────────────────┐
                    │      BTreeMap       │
                    └─────────────────────┘

    Result: 45K ops/sec regardless of thread count (no scaling)


M4: Operations shard by RunId
=============================

    Thread 1 (RunId A)    Thread 2 (RunId B)    Thread 3 (RunId C)
          │                      │                      │
          │                      │                      │
          ▼                      ▼                      ▼
    ┌──────────────┐      ┌──────────────┐      ┌──────────────┐
    │ Shard A Lock │      │ Shard B Lock │      │ Shard C Lock │
    └──────────────┘      └──────────────┘      └──────────────┘
          │                      │                      │
          ▼                      ▼                      ▼
    ┌──────────────┐      ┌──────────────┐      ┌──────────────┐
    │  HashMap A   │      │  HashMap B   │      │  HashMap C   │
    └──────────────┘      └──────────────┘      └──────────────┘

    Result: Near-linear scaling for disjoint runs


Why RunId Sharding (not key hash or type):
==========================================

    +-------------------+--------------------+----------------------+
    |  Strategy         | Pros               | Cons                 |
    +-------------------+--------------------+----------------------+
    | By RunId          | Natural agent      | Cross-run queries    |
    | (CHOSEN)          | partitioning, no   | slower               |
    |                   | coordination       |                      |
    +-------------------+--------------------+----------------------+
    | By Key Hash       | Even distribution  | Hot keys contend,    |
    |                   |                    | no semantic meaning  |
    +-------------------+--------------------+----------------------+
    | By TypeTag        | Primitive          | Doesn't help         |
    |                   | isolation          | multi-run scaling    |
    +-------------------+--------------------+----------------------+

    RunId wins because:
    1. Agents naturally partition by run
    2. Cross-run queries are rare
    3. No coordination needed between runs
    4. Enables future per-run WAL segments


Scaling Targets:
================

    +----------+---------------------------+
    | Threads  | Disjoint Key Scaling      |
    +----------+---------------------------+
    |    1     | Baseline                  |
    |    2     | ≥ 1.8× of 1-thread        |
    |    4     | ≥ 3.2× of 1-thread        |
    |    8     | ≥ 6.0× of 1-thread        |
    +----------+---------------------------+
```

---

## 5. Transaction Object Pooling

```
+-------------------------------------------------------------------------+
|                    Transaction Object Pooling (M4)                       |
+-------------------------------------------------------------------------+

M3: Allocate on every transaction
=================================

    begin_transaction()
          │
          ▼
    ┌──────────────────────────────────────┐
    │  TransactionContext {                │
    │    run_id,                           │    ← heap alloc
    │    snapshot: clone(),                │    ← heap alloc
    │    read_set: HashMap::new(),         │    ← heap alloc
    │    write_set: HashMap::new(),        │    ← heap alloc
    │  }                                   │
    └──────────────────────────────────────┘
          │
          ▼
      Use txn...
          │
          ▼
    ┌──────────────────────────────────────┐
    │           drop(txn)                  │    ← deallocate all
    └──────────────────────────────────────┘

    Cost: ~50-100ns per allocation × 4 = 200-400ns overhead


M4: Thread-local pool reuses allocations
========================================

                    Thread Local Storage
    ┌─────────────────────────────────────────────────────┐
    │   TXN_POOL: RefCell<Vec<TransactionContext>>        │
    │                                                     │
    │   ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐                  │
    │   │ txn │ │ txn │ │ txn │ │ ... │  (up to 8)       │
    │   └─────┘ └─────┘ └─────┘ └─────┘                  │
    └─────────────────────────────────────────────────────┘
          ▲                         │
          │                         │
          │ return_to_pool()        │ pop()
          │                         │
          │                         ▼
    ┌──────────────────────────────────────┐
    │  begin_transaction() {               │
    │    match pool.pop() {                │
    │      Some(txn) => txn.reset(),       │    ← reuse, no alloc
    │      None => TransactionContext      │
    │               ::new()                │    ← only if pool empty
    │    }                                 │
    │  }                                   │
    └──────────────────────────────────────┘
          │
          ▼
      Use txn...
          │
          ▼
    ┌──────────────────────────────────────┐
    │  end_transaction(txn) {              │
    │    if pool.len() < MAX_POOL_SIZE {   │
    │      pool.push(txn)                  │    ← return to pool
    │    }                                 │
    │  }                                   │
    └──────────────────────────────────────┘


reset() - Clear without deallocating:
=====================================

    impl TransactionContext {
        fn reset(&mut self, run_id, snapshot, version) {
            self.run_id = run_id;
            self.snapshot = snapshot;
            self.read_set.clear();    // capacity preserved
            self.write_set.clear();   // capacity preserved
            self.version = version;
        }
    }


Result:
=======

    core/put_hot (with alloc):       ~1700ns
    core/put_hot_prealloc (pooled):  ~887ns
                                     ─────────
    Savings:                         ~2× faster
```

---

## 6. Read Path Optimization

```
+-------------------------------------------------------------------------+
|                      Read Path Optimization (M4)                         |
+-------------------------------------------------------------------------+

M3: Full transaction overhead for reads
=======================================

    kv.get(run_id, key)
          │
          ▼
    ┌─────────────────────────────────────────┐
    │ 1. Transaction begin                    │  ← allocations
    │    - Create snapshot                    │
    │    - Create read_set                    │
    │    - Create write_set                   │
    └─────────────────────────────────────────┘
          │
          ▼
    ┌─────────────────────────────────────────┐
    │ 2. Key lookup in storage                │  ← actual work
    └─────────────────────────────────────────┘
          │
          ▼
    ┌─────────────────────────────────────────┐
    │ 3. Read-set recording                   │  ← unnecessary
    └─────────────────────────────────────────┘
          │
          ▼
    ┌─────────────────────────────────────────┐
    │ 4. Transaction commit                   │  ← unnecessary
    │    - Validation                         │
    │    - WAL append                         │
    └─────────────────────────────────────────┘
          │
          ▼
      Result: 139µs (target: <5µs)


M4: Fast path for reads
=======================

    kv.get(run_id, key)
          │
          ▼
    ┌─────────────────────────────────────────┐
    │ 1. Acquire snapshot                     │  ← Arc bump only
    │    Snapshot {                           │
    │      version: atomic.load()             │
    │      store: Arc::clone(&store)          │
    │    }                                    │
    └─────────────────────────────────────────┘
          │
          ▼
    ┌─────────────────────────────────────────┐
    │ 2. Direct key lookup                    │  ← actual work
    │    snapshot.get(&key)                   │
    └─────────────────────────────────────────┘
          │
          ▼
      Result: <5µs (28× faster)


What we SKIP:                   What we KEEP:
============                    =============

✗ Transaction allocation        ✓ Snapshot isolation
✗ Read-set recording            ✓ Consistent view
✗ Write-set creation            ✓ Run isolation (key prefix)
✗ Commit validation
✗ WAL append


When Fast Path is Safe:
=======================

    +--------------------+--------+--------------------------------+
    | Operation          | Safe?  | Reason                         |
    +--------------------+--------+--------------------------------+
    | Single-key read    |   ✓    | No write-write conflicts       |
    | Multi-key read     |   ✓    | Snapshot provides consistency  |
    | Read-then-write    |   ✗    | Need txn for atomicity         |
    | CAS                |   ✗    | Need version tracking          |
    +--------------------+--------+--------------------------------+


HARD INVARIANT:
===============

    ┌─────────────────────────────────────────────────────────────┐
    │  All fast-path reads must be OBSERVATIONALLY EQUIVALENT    │
    │  to a snapshot-based transaction.                           │
    │                                                             │
    │  • No dirty reads                                           │
    │  • No stale reads                                           │
    │  • No torn reads                                            │
    │                                                             │
    │  Any optimization that changes visibility, ordering, or     │
    │  consistency is FORBIDDEN.                                  │
    └─────────────────────────────────────────────────────────────┘


Snapshot Acquisition Requirements:
==================================

    +------------------------------------+---------------------------+
    | Requirement                        | Rationale                 |
    +------------------------------------+---------------------------+
    | Must be allocation-free            | ~50-100ns per alloc kills |
    |                                    | throughput                |
    +------------------------------------+---------------------------+
    | Must NOT acquire global locks      | Lock contention destroys  |
    |                                    | scaling                   |
    +------------------------------------+---------------------------+
    | Must NOT scan data structures      | O(n) snapshot = unusable  |
    +------------------------------------+---------------------------+
    | Must NOT touch WAL                 | WAL is for writes only    |
    +------------------------------------+---------------------------+

    // GOOD: O(1), allocation-free
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            version: self.version.load(Acquire),
            store: Arc::clone(&self.store),  // Arc bump only
        }
    }

    // BAD: O(n), allocates
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            data: self.store.read().clone(),  // Full clone!
        }
    }
```

---

## 7. Buffered Mode Flush Architecture

```
+-------------------------------------------------------------------------+
|                   Buffered Mode Flush Architecture                       |
+-------------------------------------------------------------------------+

Transaction Flow in Buffered Mode:
==================================

    Application Thread                    Background Flush Thread
          │                                        │
          ▼                                        │
    ┌───────────────────┐                          │
    │ commit() {        │                          │
    │   wal_buffer      │                          │
    │     .append()     │ ─── memory only ───►     │
    │   storage.apply() │                          │
    │   if should_flush │                          │
    │     flush_async() │ ── signal ──────────────►│
    │ }                 │                          │
    └───────────────────┘                          │
          │                                        │
          ▼                                        │
      return Ok(())                                │
     (not waiting)                                 │
                                                   ▼
                                          ┌───────────────────┐
                                          │ wait_for_signal() │
                                          │ if shutdown:      │
                                          │   final_flush()   │
                                          │   return          │
                                          │ wal.fsync()       │
                                          └───────────────────┘


Thread Lifecycle (CRITICAL):
============================

    BufferedDurability {
        shutdown: AtomicBool,              // Shutdown signal
        flush_thread: Option<JoinHandle>,  // Thread handle
    }

    Drop::drop() {
        shutdown.store(true);              // Signal thread to exit
        flush_signal.notify_all();         // Wake up thread
        flush_thread.join();               // Wait for thread
    }


Flush Trigger Conditions:
=========================

    should_flush() returns true when:

    ┌─────────────────────────────────────────────────────────────┐
    │                                                             │
    │   pending_writes >= max_pending_writes                      │
    │                 OR                                          │
    │   time_since_last_flush >= flush_interval_ms                │
    │                                                             │
    └─────────────────────────────────────────────────────────────┘

    Typical configuration:
    - flush_interval_ms: 50-100ms
    - max_pending_writes: 500-1000


Data Loss Window:
=================

    Clean Shutdown              Crash (no graceful shutdown)
    ═══════════════            ══════════════════════════════

    ┌──────────────┐           ┌──────────────┐
    │ shutdown() { │           │              │
    │   flush_sync │           │  LOST DATA:  │
    │ }            │           │  - Pending   │
    └──────────────┘           │    buffer    │
          │                    │  - Up to     │
          ▼                    │    interval  │
    All data safe              └──────────────┘


Recovery Behavior:
==================

    ┌────────────────┬────────────────────┬──────────────────────┐
    │    Mode        │ Clean Shutdown     │ Crash Recovery       │
    ├────────────────┼────────────────────┼──────────────────────┤
    │ InMemory       │ Data lost          │ Data lost            │
    │ Buffered       │ WAL flushed, safe  │ Up to interval lost  │
    │ Strict         │ Data safe          │ Data safe            │
    └────────────────┴────────────────────┴──────────────────────┘
```

---

## 8. Performance Targets

```
+-------------------------------------------------------------------------+
|                       Performance Targets (M4)                           |
+-------------------------------------------------------------------------+

Latency Targets:
================

    ┌────────────────────┬────────────┬────────────┬────────────┐
    │    Operation       │  InMemory  │  Buffered  │   Strict   │
    ├────────────────────┼────────────┼────────────┼────────────┤
    │ engine/get_direct  │   <500ns   │   <500ns   │   <500ns   │
    │ engine/put_direct  │   <3µs     │   <20µs    │   ~2ms     │
    │ kvstore/get        │   <5µs     │   <5µs     │   <5µs     │
    │ kvstore/put        │   <8µs     │   <30µs    │   ~2ms     │
    │ eventlog/append    │   <10µs    │   <40µs    │   ~3ms     │
    └────────────────────┴────────────┴────────────┴────────────┘


Throughput Targets (InMemory mode):
===================================

    Scenario                        Target
    ────────────────────────────────────────────────────
    1-thread, hot key               ≥ 250K ops/sec
    1-thread, uniform random        ≥ 200K ops/sec
    4-thread, disjoint keys         ≥ 800K ops/sec  (3.2×)
    4-thread, same key              ≥ 25% of 1-thread
    8-thread, disjoint keys         ≥ 1.4M ops/sec  (5.6×)


Comparison to Redis:
====================

    ┌─────────────────────────┬──────────────────────────────────┐
    │        System           │        Throughput                │
    ├─────────────────────────┼──────────────────────────────────┤
    │ Redis over TCP          │ ~100K-200K ops/sec (network)     │
    │ Redis internal loop     │ Millions ops/sec                 │
    │ M4 InMemory (target)    │ 250K ops/sec (removes blockers)  │
    │ M5+ (goal)              │ Millions ops/sec (Redis parity)  │
    └─────────────────────────┴──────────────────────────────────┘

    M4 is a DE-BLOCKING milestone, not Redis parity.


Facade Tax Targets:
===================

    ┌─────────┬──────────┬─────────────┬─────────────────────────┐
    │  Ratio  │  Target  │ M3 Measured │ Notes                   │
    ├─────────┼──────────┼─────────────┼─────────────────────────┤
    │  A1/A0  │   <10×   │   ~2400×    │ Fixed by InMemory mode  │
    │  B/A1   │   <5×    │   ~10×      │ Improved by read opt    │
    │  B/A0   │   <30×   │   ~4000×    │ Fixed by above          │
    └─────────┴──────────┴─────────────┴─────────────────────────┘

    Where:
    - A0 = Core data structure operations (BTreeMap/HashMap)
    - A1 = Engine layer (Database.put_direct)
    - B  = Facade layer (KVStore.put)


Performance Transformation:
===========================

    ┌──────────────────┬────────────┬──────────────┬─────────────┐
    │     Metric       │     M3     │ M4 InMemory  │ Improvement │
    ├──────────────────┼────────────┼──────────────┼─────────────┤
    │ kvstore/put      │   2.2 ms   │    <8 µs     │    275×     │
    │ kvstore/get      │   139 µs   │    <5 µs     │     28×     │
    │ Throughput       │  475 ops/s │  250K ops/s  │    500×     │
    └──────────────────┴────────────┴──────────────┴─────────────┘
```

---

## 9. Red Flags and Hard Stop Criteria

```
+-------------------------------------------------------------------------+
|                 Red Flags / Hard Stop Criteria (M4)                      |
+-------------------------------------------------------------------------+

If ANY of these are true after M4, STOP and redesign:
=====================================================

    ┌────────────────────────┬─────────────────────┬─────────────────────┐
    │        Metric          │   Red Flag Threshold │      Action         │
    ├────────────────────────┼─────────────────────┼─────────────────────┤
    │ Snapshot acquisition   │       > 2µs         │ Redesign snapshot   │
    │ A1/A0 ratio            │       > 20×         │ Remove abstractions │
    │ B/A1 ratio             │       > 8×          │ Inline facade logic │
    │ Disjoint scaling (4T)  │       < 2.5×        │ Redesign sharding   │
    │ p99 latency            │       > 20× mean    │ Fix tail latency    │
    │ Hot-path allocations   │       > 0           │ Eliminate allocs    │
    └────────────────────────┴─────────────────────┴─────────────────────┘

    These are NOT NEGOTIABLE.


Hot Path Purity (Syscall-Free):
===============================

    Tier A0, A1, and B hot paths must NOT:

    ┌─────────────────────────────────────────────────────────────┐
    │                                                             │
    │    ✗  Perform syscalls (including time(), rand())          │
    │    ✗  Touch filesystem                                      │
    │    ✗  Perform logging                                       │
    │    ✗  Allocate heap memory                                  │
    │    ✗  Use trait objects or dynamic dispatch                 │
    │    ✗  Trigger page faults                                   │
    │                                                             │
    └─────────────────────────────────────────────────────────────┘

    Enforced by: Code review + benchmark validation


Facade Tax Enforcement:
=======================

    ┌─────────────────────────────────────────────────────────────┐
    │                                                             │
    │  Any layer with B/A1 > 5× must provide written              │
    │  justification in the PR explaining:                        │
    │                                                             │
    │    1. WHY the overhead is necessary                         │
    │    2. WHAT would be required to reduce it                   │
    │                                                             │
    └─────────────────────────────────────────────────────────────┘
```

---

## 10. Perf-Guided Development Loop

```
+-------------------------------------------------------------------------+
|                  Perf-Guided Development Loop (M4)                       |
+-------------------------------------------------------------------------+

Every M4 optimization MUST follow this loop:
============================================

    ┌─────────────┐
    │  IDENTIFY   │   Find hot function via `perf record` / flamegraph
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐
    │ HYPOTHESIZE │   State expected improvement and magnitude
    └──────┬──────┘   "Removing clone() will reduce kvstore/get by 50%"
           │
           ▼
    ┌─────────────┐
    │  IMPLEMENT  │   Make the change
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐
    │  BENCHMARK  │   Run targeted benchmark (not full suite)
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐
    │   COMPARE   │   Compare to m3_baseline_perf tag
    └──────┬──────┘
           │
           ▼
    ┌─────────────┐
    │   DECIDE    │   Keep if improved, REVERT if not
    └─────────────┘


Rules:
======

    ┌─────────────────────────────────────────────────────────────┐
    │                                                             │
    │  ✗  No speculative optimizations                            │
    │     ("this should be faster")                               │
    │                                                             │
    │  ✗  No premature abstractions                               │
    │     ("we might need this later")                            │
    │                                                             │
    │  ✗  No changes without before/after numbers                 │
    │                                                             │
    │  ✓  All changes must be justified by data                   │
    │                                                             │
    └─────────────────────────────────────────────────────────────┘


Example Documentation:
======================

    ┌─────────────────────────────────────────────────────────────┐
    │ Hypothesis: "Removing clone() in read path will reduce      │
    │             kvstore/get by 50%"                             │
    │                                                             │
    │ Before:      139µs                                          │
    │ After:       68µs                                           │
    │ Improvement: 51%                                            │
    │ Decision:    KEEP                                           │
    └─────────────────────────────────────────────────────────────┘
```

---

## 11. M4 Data Flow: Put Operation

```
+-------------------------------------------------------------------------+
|                  M4 Data Flow: Put Operation by Mode                     |
+-------------------------------------------------------------------------+

InMemory Mode:
==============

    Application             KVStore           Database          ShardedStore
        │                      │                  │                   │
        │ kv.put(run_id,      │                  │                   │
        │       key, value)   │                  │                   │
        ├─────────────────────►│                  │                   │
        │                      │                  │                   │
        │                      │ get_pooled_txn() │                   │
        │                      ├─────────────────►│                   │
        │                      │◄─────────────────┤                   │
        │                      │ (from pool)      │                   │
        │                      │                  │                   │
        │                      │ txn.put(key,val) │                   │
        │                      │─ ─ ─ ─ ─ ─ ─ ─ ─►│                   │
        │                      │ (buffer in       │                   │
        │                      │  write_set)      │                   │
        │                      │                  │                   │
        │                      │ commit()         │                   │
        │                      │─ ─ ─ ─ ─ ─ ─ ─ ─►│                   │
        │                      │                  │ ┌─────────────────┐
        │                      │                  │ │ NO WAL          │
        │                      │                  │ │ NO fsync        │
        │                      │                  │ └─────────────────┘
        │                      │                  │                   │
        │                      │                  │ apply(write_set)  │
        │                      │                  ├──────────────────►│
        │                      │                  │                   │
        │                      │                  │ return_to_pool()  │
        │                      │                  │                   │
        │◄─────────────────────┤◄─────────────────┤                   │
        │      Ok(())          │                  │                   │
        │                      │                  │                   │
        │   Total: <8µs        │                  │                   │


Strict Mode:
============

    Application             KVStore           Database          ShardedStore
        │                      │                  │                   │
        │ kv.put(run_id,      │                  │                   │
        │       key, value)   │                  │                   │
        ├─────────────────────►│                  │                   │
        │                      │                  │                   │
        │                      │ get_pooled_txn() │                   │
        │                      ├─────────────────►│                   │
        │                      │◄─────────────────┤                   │
        │                      │                  │                   │
        │                      │ txn.put()        │                   │
        │                      │─ ─ ─ ─ ─ ─ ─ ─ ─►│                   │
        │                      │                  │                   │
        │                      │ commit()         │                   │
        │                      │─ ─ ─ ─ ─ ─ ─ ─ ─►│                   │
        │                      │                  │ ┌─────────────────┐
        │                      │                  │ │ WAL append      │
        │                      │                  │ │ fsync() ◄─ SLOW │
        │                      │                  │ └─────────────────┘
        │                      │                  │                   │
        │                      │                  │ apply(write_set)  │
        │                      │                  ├──────────────────►│
        │                      │                  │                   │
        │◄─────────────────────┤◄─────────────────┤                   │
        │      Ok(())          │                  │                   │
        │                      │                  │                   │
        │   Total: ~2ms        │                  │                   │
```

---

## 12. M4 Data Flow: Get Operation (Fast Path)

```
+-------------------------------------------------------------------------+
|                M4 Data Flow: Get Operation (Fast Path)                   |
+-------------------------------------------------------------------------+

M4 Fast Path (Read Optimization):
=================================

    Application             KVStore           Database          ShardedStore
        │                      │                  │                   │
        │ kv.get(run_id, key) │                  │                   │
        ├─────────────────────►│                  │                   │
        │                      │                  │                   │
        │                      │ db.snapshot()    │                   │
        │                      ├─────────────────►│                   │
        │                      │                  │ Snapshot {        │
        │                      │                  │   version: load() │
        │                      │                  │   store: Arc      │
        │                      │                  │          ::clone()│
        │                      │                  │ }                 │
        │                      │◄─────────────────┤  (< 500ns)        │
        │                      │                  │                   │
        │                      │ snapshot.get(key)│                   │
        │                      │─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─►│
        │                      │                  │                   │
        │                      │◄─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┤
        │                      │   Some(value)    │                   │
        │                      │                  │                   │
        │◄─────────────────────┤                  │                   │
        │      Some(value)     │                  │                   │
        │                      │                  │                   │
        │   Total: <5µs        │                  │                   │


What We SKIP (vs M3):
=====================

    ┌─────────────────────────────────────────────────────────────┐
    │                                                             │
    │  ✗  Transaction object allocation                           │
    │  ✗  Read-set recording                                      │
    │  ✗  Write-set creation                                      │
    │  ✗  Commit validation                                       │
    │  ✗  WAL append                                              │
    │                                                             │
    └─────────────────────────────────────────────────────────────┘


What We KEEP:
=============

    ┌─────────────────────────────────────────────────────────────┐
    │                                                             │
    │  ✓  Snapshot isolation (consistent view)                    │
    │  ✓  Run isolation (key prefixing)                           │
    │  ✓  Observational equivalence to full transaction           │
    │                                                             │
    └─────────────────────────────────────────────────────────────┘
```

---

## 13. Layer Dependencies (M4 Updated)

```
+-------------------------------------------------------------------------+
|                      Dependency Graph (M4)                               |
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
                   ┌────────────────────────────────┐
                   │           Engine               │
                   │        (database.rs)           │
                   │                                │
                   │  M4 NEW:                       │
                   │  - DurabilityMode selection    │
                   │  - Transaction pooling         │
                   │  - Read path optimization      │
                   └───────────────┬────────────────┘
                                   |
             +---------------------+---------------------+
             |                     |                     |
        depends on            depends on            depends on
             |                     |                     |
             v                     v                     v
    +------------------+  +-------------------+  +-------------------+
    |   Storage (M4)   |  |  Durability (M4)  |  |  Concurrency (M4) |
    |    UPDATED       |  |     UPDATED       |  |     UPDATED       |
    |                  |  |                   |  |                   |
    | - ShardedStore   |  | - InMemoryMode    |  | - TxnPool         |
    | - DashMap        |  | - BufferedMode    |  | - ReadFastPath    |
    | - FxHashMap      |  | - StrictMode      |  | - LockSharding    |
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


M4 Structural Changes:
======================

    crates/
    ├── core/                      (unchanged)
    ├── engine/
    │   ├── database.rs            (updated: durability selection)
    │   ├── storage/
    │   │   ├── unified.rs         (M3: global RwLock)
    │   │   └── sharded.rs         (M4 NEW: DashMap + HashMap)
    │   ├── durability/
    │   │   ├── wal.rs             (existing)
    │   │   └── modes.rs           (M4 NEW: InMemory/Buffered/Strict)
    │   └── transaction/
    │       ├── context.rs         (updated: pooling support)
    │       └── pool.rs            (M4 NEW: thread-local pools)
    └── primitives/                (unchanged - benefits automatically)
```

---

## 14. M4 Philosophy

```
+-------------------------------------------------------------------------+
|                          M4 Philosophy                                   |
+-------------------------------------------------------------------------+

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │     M4 does not aim to be fast.                                     │
    │                                                                     │
    │     M4 aims to be FASTABLE.                                         │
    │                                                                     │
    │     M4 is explicitly allowed to be slow relative to Redis.          │
    │     M4 only ensures the architecture CAN be made fast later.        │
    │                                                                     │
    │     Do not rationalize "good enough" at M4 completion.              │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


M4 is a De-Blocking Milestone:
==============================

    ┌─────────────────────────────────────────────────────────────────────┐
    │                                                                     │
    │  BEFORE M4:                                                         │
    │  ───────────                                                        │
    │  Architectural blockers prevent reaching Redis-class latency.       │
    │  fsync on every write = 500× slower than necessary.                 │
    │  Global lock = no scaling.                                          │
    │                                                                     │
    │  AFTER M4:                                                          │
    │  ──────────                                                         │
    │  Blockers removed. Path to Redis parity is POSSIBLE.                │
    │  Still requires M5+ work on data layout, cache, lock-free.          │
    │                                                                     │
    │  M4 is NOT the end state. It is the enabling state.                 │
    │                                                                     │
    └─────────────────────────────────────────────────────────────────────┘


What M4 Provides vs What M5+ Must Provide:
==========================================

    ┌────────────────────────────┬────────────────────────────────────────┐
    │       M4 Provides          │       M5+ Must Provide                 │
    ├────────────────────────────┼────────────────────────────────────────┤
    │ Durability modes           │ Arena allocators                       │
    │ Transaction pooling        │ Cache-line alignment                   │
    │ Lock sharding (DashMap)    │ Lock-free reads (epoch/RCU)            │
    │ Read fast path             │ SoA transforms                         │
    │ Performance instrumentation│ Prefetching                            │
    │ Facade tax validation      │ Contention backoff                     │
    │                            │ Conflict detection optimization        │
    └────────────────────────────┴────────────────────────────────────────┘

    M5+ items are REQUIRED for Redis parity.
    They are DEFERRED, not ABANDONED.
```

---

These diagrams illustrate the key architectural components and flows for M4's Performance milestone. M4 builds upon M3's Primitives while removing architectural blockers that prevent reaching Redis-class latency.

**Key Design Points Reflected in These Diagrams**:
- M4 introduces three durability modes with different performance/durability tradeoffs
- DashMap + HashMap replaces RwLock + BTreeMap (tactical improvement, not final)
- Lock sharding by RunId enables multi-thread scaling for disjoint runs
- Transaction object pooling eliminates allocation overhead on hot path
- Read fast path bypasses full transaction overhead while preserving isolation
- Red flags and hard stop criteria define non-negotiable thresholds
- M4 is a de-blocking milestone, not Redis parity—that requires M5+ work

**M4 Philosophy**: M4 does not aim to be fast. M4 aims to be *fastable*.
