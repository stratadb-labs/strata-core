# M2 Architecture Diagrams: Transactions & OCC

This document contains visual representations of the M2 architecture with Optimistic Concurrency Control.

---

## 1. System Architecture Overview (M2)

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Application Layer                           │
│                    (Agent Applications using DB)                     │
└────────────────────────────────┬────────────────────────────────────┘
                                 │
                                 │ API calls (M1 + M2)
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
│  │              │  │              │  │              │            │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘            │
│         │                 │                  │                     │
│         └─────────────────┼──────────────────┘                     │
│                           │                                        │
│                    Can use explicit transactions (M2)              │
│                                                                     │
│  * EventLog, Trace deferred to M3                                  │
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
│  │  M1 Responsibilities:                                     │    │
│  │  • Run Lifecycle (begin_run, end_run)                    │    │
│  │  • Operation Coordination (put, get, delete)             │    │
│  │  • Atomic Updates (Storage + WAL)                        │    │
│  │  • Recovery on Startup                                   │    │
│  │                                                           │    │
│  │  M2 Responsibilities (NEW):                               │    │
│  │  • Transaction Management (begin_transaction)            │    │
│  │  • Snapshot Creation                                     │    │
│  │  • Conflict Detection & Retry                            │    │
│  │                                                           │    │
│  │  State:                                                   │    │
│  │  • storage: Arc<UnifiedStore>                            │    │
│  │  • wal: Arc<Mutex<WAL>>                                  │    │
│  │  • run_tracker: Arc<RunTracker>                          │    │
│  │  • next_txn_id: AtomicU64                                │    │
│  └───────────────────────────────────────────────────────────┘    │
│                             │                                       │
│                             │ Calls all layers                      │
│                             ▼                                       │
│              ┌──────────────┴──────┬─────────────────┐             │
└──────────────┼─────────────────────┼─────────────────┼─────────────┘
               │                     │                 │
               ▼                     ▼                 ▼
┌──────────────────────┐  ┌────────────────────────┐  ┌──────────────────────┐
│  Storage Layer (M1)  │  │  Durability Layer (M1) │  │ Concurrency (M2 NEW) │
│                      │  │                        │  │                      │
│  ┌────────────────┐ │  │  ┌──────────────────┐ │  │  ┌────────────────┐ │
│  │ UnifiedStore   │ │  │  │      WAL         │ │  │  │ Transaction    │ │
│  │                │ │  │  │                  │ │  │  │ Context        │ │
│  │ • BTreeMap     │ │  │  │ • Append-only    │ │  │  │                │ │
│  │ • RwLock       │ │  │  │ • 3 modes        │ │  │  │ • read_set     │ │
│  │ • Indices      │ │  │  │ • CRC32          │ │  │  │ • write_set    │ │
│  │ • Versioning   │ │  │  └──────────────────┘ │  │  │ • cas_set      │ │
│  └────────────────┘ │  │                        │  │  │ • snapshot     │ │
│                      │  │  ┌──────────────────┐ │  │  └────────────────┘ │
│  M2 Addition:        │  │  │    Recovery      │ │  │                      │
│  • clone_data_at_    │  │  │                  │ │  │  ┌────────────────┐ │
│    version()         │  │  │ • WAL replay     │ │  │  │ Snapshot Mgmt  │ │
│                      │  │  │ • Validation     │ │  │  │                │ │
│                      │  │  └──────────────────┘ │  │  │ • Cloned View  │ │
└──────────────────────┘  └────────────────────────┘  │  │ • Version      │ │
                                                      │  │   tracking     │ │
                                                      │  └────────────────┘ │
                                                      │                      │
                                                      │  ┌────────────────┐ │
                                                      │  │ Validation     │ │
                                                      │  │                │ │
                                                      │  │ • Read checks  │ │
                                                      │  │ • Write checks │ │
                                                      │  │ • CAS checks   │ │
                                                      │  └────────────────┘ │
                                                      └──────────────────────┘
                                                                 │
                                                                 │ Uses
                                                                 ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          Core Types Layer (M1)                      │
│                      (Foundation Definitions)                       │
│                                                                     │
│  Types:                                                             │
│  • RunId, Namespace, Key, TypeTag, Value, VersionedValue           │
│                                                                     │
│  Traits:                                                            │
│  • Storage        - Storage abstraction                             │
│  • SnapshotView   - Snapshot abstraction (M2 implements)           │
│                                                                     │
│  Errors:                                                            │
│  • Error          - Top-level error enum                            │
│  • ConcurrencyError - M2 NEW: Conflict errors                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 2. OCC Transaction Flow: Multi-Step Transaction

```
┌──────────┐
│   App    │
└────┬─────┘
     │
     │ db.begin_transaction(run_id)
     ▼
┌──────────────────────────────────────────────────────────────┐
│                     Database.begin_transaction()             │
│                                                              │
│  1. Allocate txn_id (AtomicU64::fetch_add)                  │
│  2. Get current_version from storage                        │
│  3. Create snapshot (clone BTreeMap at current_version)     │
│  4. Return TransactionContext                               │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                 TransactionContext Created                      │
│                                                                 │
│  txn_id: 42                                                     │
│  run_id: <uuid>                                                 │
│  start_version: 100                                             │
│  snapshot: ClonedSnapshotView { version: 100, data: BTreeMap } │
│  read_set: {}                                                   │
│  write_set: {}                                                  │
│  cas_set: []                                                    │
│  status: Active                                                 │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         ▼
            ┌────────────────────────┐
            │  App executes txn      │
            │                        │
            │  txn.get(key_x)        │ ← READ from snapshot
            │  txn.put(key_y, val)   │ ← WRITE to write_set
            │  txn.cas(key_z, ...)   │ ← CAS to cas_set
            └────────┬───────────────┘
                     │
                     │ txn.commit()
                     ▼
┌─────────────────────────────────────────────────────────────────┐
│               TransactionContext.commit()                       │
│                                                                 │
│  Phase 1: VALIDATE                                              │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │  For each (key, read_version) in read_set:               │ │
│  │    current = storage.get(key)                             │ │
│  │    if current.version != read_version:                    │ │
│  │      CONFLICT! Abort transaction                          │ │
│  │                                                            │ │
│  │  For each key in write_set:                               │ │
│  │    if key in read_set:                                    │ │
│  │      if storage.get(key).version > start_version:         │ │
│  │        CONFLICT! Abort transaction                        │ │
│  │                                                            │ │
│  │  For each cas_op in cas_set:                              │ │
│  │    if storage.get(key).version != expected_version:       │ │
│  │      CONFLICT! Abort transaction                          │ │
│  └───────────────────────────────────────────────────────────┘ │
│                                                                 │
│  If conflicts found:                                            │
│    return Err(ConcurrencyError::WriteConflict)                 │
│                                                                 │
│  Phase 2: APPLY (if validation passed)                         │
│  ┌───────────────────────────────────────────────────────────┐ │
│  │  Acquire WAL lock                                         │ │
│  │                                                            │ │
│  │  Write to WAL:                                            │ │
│  │    ├─ BeginTxn(42, run_id, timestamp)                     │ │
│  │    ├─ Write(key_y, value, version=101) ← from write_set  │ │
│  │    ├─ Write(key_z, value, version=102) ← from cas_set    │ │
│  │    └─ CommitTxn(42, run_id)                               │ │
│  │                                                            │ │
│  │  Apply to storage:                                        │ │
│  │    ├─ storage.put(key_y, value) → version 101             │ │
│  │    └─ storage.put(key_z, value) → version 102             │ │
│  │                                                            │ │
│  │  Release WAL lock                                         │ │
│  └───────────────────────────────────────────────────────────┘ │
│                                                                 │
│  Return Ok(())                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. Snapshot Isolation Mechanism

```
┌─────────────────────────────────────────────────────────────┐
│              Timeline: Snapshot Isolation                   │
└─────────────────────────────────────────────────────────────┘

Timeline:
─────────────────────────────────────────────────────────────►
      t0        t1         t2         t3         t4

Global Version Counter:
      100       101        102        103        104

┌──────────────────────────────────────────────────────────────┐
│ t0: Initial state                                            │
│     Storage: { x: (10, v99), y: (20, v98) }                 │
│     Global version: 100                                      │
└──────────────────────────────────────────────────────────────┘
      │
      │ T1 begins
      ▼
┌──────────────────────────────────────────────────────────────┐
│ t1: Transaction T1 starts                                    │
│                                                              │
│     T1.start_version = 100                                   │
│     T1.snapshot = clone of storage at v100                   │
│       { x: (10, v99), y: (20, v98) }                        │
│                                                              │
│     T1 is now isolated from future writes                    │
└──────────────────────────────────────────────────────────────┘
      │
      │ T2 begins and commits
      ▼
┌──────────────────────────────────────────────────────────────┐
│ t2: Transaction T2 (concurrent)                              │
│                                                              │
│     T2 begins, start_version = 101                           │
│     T2.put(x, 30)                                            │
│     T2 commits → x gets version 101                          │
│                                                              │
│     Storage: { x: (30, v101), y: (20, v98) }                │
│     Global version: 101                                      │
│                                                              │
│     T1 DOES NOT SEE this change (still sees snapshot v100)  │
└──────────────────────────────────────────────────────────────┘
      │
      │ T1 reads x
      ▼
┌──────────────────────────────────────────────────────────────┐
│ t3: T1 reads x                                               │
│                                                              │
│     T1.get(x) → reads from snapshot                          │
│       → returns (10, v99)  ← OLD VALUE                       │
│                                                              │
│     T1.read_set = { x → v99 }                                │
│                                                              │
│     Snapshot isolation: T1 sees consistent view at v100      │
└──────────────────────────────────────────────────────────────┘
      │
      │ T1 writes y
      ▼
┌──────────────────────────────────────────────────────────────┐
│ t4: T1 writes y and commits                                  │
│                                                              │
│     T1.put(y, 25) → buffered in write_set                    │
│     T1.commit()                                              │
│                                                              │
│     Validation:                                              │
│       read_set: { x → v99 }                                  │
│       Current storage: x is v101 (changed!)                  │
│       CONFLICT DETECTED!                                     │
│                                                              │
│     Result: T1 ABORTS                                        │
│                                                              │
│     Why: x changed from v99 to v101 after T1 read it         │
└──────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                    Key Insight                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  • T1's snapshot is FROZEN at version 100                   │
│  • T1 sees all data <= version 100                          │
│  • T1 NEVER sees T2's writes (version 101)                  │
│  • At commit, T1 validates: "did anything I read change?"   │
│  • If yes → ABORT (conflict)                                │
│  • If no  → COMMIT (safe)                                   │
│                                                             │
│  This provides SNAPSHOT ISOLATION:                          │
│    - Repeatable reads within transaction                    │
│    - No dirty reads                                         │
│    - Conflicts detected at commit time                      │
└─────────────────────────────────────────────────────────────┘
```

---

## 4. Conflict Detection Examples

### 4.1 Scenario: Read-Write Conflict (ABORT)

```
Transaction T1                   Storage State                 Transaction T2
══════════════                  ══════════════                 ══════════════

BEGIN (v100)
  snapshot: x=10 (v99)          x: (10, v99)
                                global_version: 100

READ x
  get(x) → 10 (v99)             x: (10, v99)
  read_set: {x → v99}
                                                               BEGIN (v100)

                                                               WRITE x=30
                                                               write_set: {x → 30}

                                                               COMMIT
                                                                 ├─ Validate (pass)
                                x: (30, v101) ← UPDATED         ├─ Apply
                                global_version: 101             └─ Success
WRITE y=25
write_set: {y → 25}

COMMIT
  ├─ Validate:                  x: (30, v101)
  │   read_set: {x → v99}
  │   Current x: v101 ← CHANGED!
  │
  └─ CONFLICT!
     ABORT T1 ✗

Final state:                    x: (30, v101)    ← T2's write
                                y: (20, v98)     ← T1 aborted, y unchanged
```

### 4.2 Scenario: Write-Write (Different Keys) - NO CONFLICT (COMMIT)

```
Transaction T1                   Storage State                 Transaction T2
══════════════                  ══════════════                 ══════════════

BEGIN (v100)                    x: (10, v99)                   BEGIN (v100)
                                y: (20, v98)
                                global_version: 100

WRITE x=15                                                     WRITE y=25
write_set: {x → 15}                                            write_set: {y → 25}

COMMIT                                                         COMMIT
  ├─ Validate                                                    ├─ Validate
  │   write_set: {x}            x: (10, v99)                    │   write_set: {y}
  │   No read conflicts         y: (20, v98)                    │   No read conflicts
  │
  ├─ Apply                      x: (15, v101) ← T1              ├─ Apply
  │                             global_version: 101             │
  └─ Success ✓                                                  │   y: (25, v102) ← T2
                                                                │   global_version: 102
                                                                └─ Success ✓

Final state:                    x: (15, v101)    ← T1's write
                                y: (25, v102)    ← T2's write
                                Both committed!
```

### 4.3 Scenario: CAS Conflict (ABORT)

```
Transaction T1                   Storage State                 Transaction T2
══════════════                  ══════════════                 ══════════════

BEGIN (v100)
  snapshot: counter=5 (v99)     counter: (5, v99)
                                global_version: 100

READ counter
  get(counter) → 5 (v99)        counter: (5, v99)
  read_set: {counter → v99}
                                                               BEGIN (v100)

                                                               READ counter
                                                                 get(counter) → 5 (v99)

CAS counter                                                    CAS counter
  expected: v99                                                  expected: v99
  new_value: 6                                                   new_value: 6
  cas_set: [{counter, v99, 6}]                                   cas_set: [{counter, v99, 6}]

                                                               COMMIT
                                                                 ├─ Validate (pass)
                                counter: (6, v101) ← UPDATED    ├─ Apply CAS
                                global_version: 101             └─ Success ✓

COMMIT
  ├─ Validate:                  counter: (6, v101)
  │   cas_set: [expected v99]
  │   Current counter: v101 ← CHANGED!
  │
  └─ CAS CONFLICT!
     ABORT T1 ✗

Final state:                    counter: (6, v101)  ← T2's CAS succeeded
                                                      T1's CAS failed
```

---

## 5. Snapshot Creation and Management

```
┌─────────────────────────────────────────────────────────────┐
│           Snapshot Creation (M2: ClonedSnapshotView)        │
└─────────────────────────────────────────────────────────────┘

Storage (UnifiedStore):
┌──────────────────────────────────────────────────────┐
│ data: RwLock<BTreeMap<Key, VersionedValue>>         │
│                                                      │
│   key_a → (value: 10, version: 95)                  │
│   key_b → (value: 20, version: 98)                  │
│   key_c → (value: 30, version: 99)                  │
│   key_d → (value: 40, version: 101) ← future write  │
│   key_e → (value: 50, version: 103) ← future write  │
│                                                      │
│ global_version: 103                                  │
└──────────────────────────────────────────────────────┘
        │
        │ begin_transaction() at version 100
        │
        ▼
┌──────────────────────────────────────────────────────┐
│ ClonedSnapshotView::create(storage, version=100)    │
│                                                      │
│ 1. Acquire read lock on storage.data                │
│ 2. Clone BTreeMap, filtering:                       │
│      - Include: v.version <= 100                    │
│      - Exclude: v.is_expired()                      │
│                                                      │
│ Result: New BTreeMap with snapshot data             │
└─────────────────────┬────────────────────────────────┘
                      │
                      ▼
┌──────────────────────────────────────────────────────┐
│ Snapshot (version=100):                              │
│                                                      │
│   key_a → (value: 10, version: 95)   ✓ included    │
│   key_b → (value: 20, version: 98)   ✓ included    │
│   key_c → (value: 30, version: 99)   ✓ included    │
│   key_d → (value: 40, version: 101)  ✗ excluded    │
│   key_e → (value: 50, version: 103)  ✗ excluded    │
│                                                      │
│ Snapshot is FROZEN at version 100                   │
└──────────────────────────────────────────────────────┘

Transaction reads from this snapshot:
┌──────────────────────────────────────────────────────┐
│ txn.get(key_a) → reads from snapshot → 10            │
│ txn.get(key_d) → NOT IN SNAPSHOT → None              │
│                                                      │
│ Even if storage is updated:                          │
│   Storage: key_a → (15, v104)  ← NEW                │
│   Snapshot: key_a → (10, v95)  ← UNCHANGED           │
│                                                      │
│ Transaction sees consistent snapshot throughout      │
└──────────────────────────────────────────────────────┘

Cost Analysis (M2):
┌──────────────────────────────────────────────────────┐
│ Memory: O(data_size) per concurrent transaction     │
│   Example: 10K keys × 1KB = 10MB per snapshot       │
│   3 concurrent txns = 30MB total                    │
│                                                      │
│ CPU: O(data_size) to clone                          │
│   Example: 10K keys, ~50ms to clone                 │
│                                                      │
│ Lifetime: Transaction duration (typically < 1 sec)   │
│                                                      │
│ Acceptable for M2 (agents: <10K keys, <5 txns)      │
└──────────────────────────────────────────────────────┘

Future Optimization (M3: LazySnapshotView):
┌──────────────────────────────────────────────────────┐
│ Instead of cloning:                                  │
│   - Store reference to live storage                  │
│   - On get(key): storage.get_versioned(key, v100)   │
│   - Version check at read time                       │
│                                                      │
│ Benefits:                                            │
│   - Zero memory overhead (no clone)                  │
│   - Instant snapshot creation (<1ms)                 │
│                                                      │
│ Trade-off:                                           │
│   - Every read hits storage (read lock contention)   │
└──────────────────────────────────────────────────────┘
```

---

## 6. Transaction State Machine

```
┌─────────────────────────────────────────────────────────────┐
│                 Transaction Lifecycle States                │
└─────────────────────────────────────────────────────────────┘

                   begin_transaction()
                           │
                           ▼
                    ┌─────────────┐
                    │   Active    │
                    └──────┬──────┘
                           │
           ┌───────────────┼───────────────┐
           │               │               │
      get()/put()     abort()         commit()
           │               │               │
           ▼               ▼               ▼
    ┌─────────────┐  ┌──────────┐  ┌──────────────┐
    │   Active    │  │ Aborted  │  │  Validating  │
    │ (continue)  │  │          │  └──────┬───────┘
    └─────────────┘  └──────────┘         │
                                           │
                                     ┌─────┴─────┐
                                     │           │
                              validation  validation
                                 pass        fail
                                     │           │
                                     ▼           ▼
                              ┌───────────┐ ┌──────────┐
                              │ Committed │ │ Aborted  │
                              └───────────┘ └──────────┘

State Transitions:
══════════════════

1. Created → Active
   ├─ Trigger: begin_transaction()
   ├─ Actions: Allocate txn_id, create snapshot
   └─ Result: TransactionContext ready

2. Active → Active
   ├─ Trigger: get(), put(), delete(), cas()
   ├─ Actions: Update read_set, write_set, cas_set
   └─ Result: Transaction continues

3. Active → Aborted
   ├─ Trigger: abort()
   ├─ Actions: Mark status, discard sets
   └─ Result: Transaction terminated (no WAL writes)

4. Active → Validating
   ├─ Trigger: commit()
   ├─ Actions: Begin validation phase
   └─ Result: Checking for conflicts

5. Validating → Committed
   ├─ Trigger: Validation passes (no conflicts)
   ├─ Actions: Write to WAL, apply to storage
   └─ Result: Transaction durably committed

6. Validating → Aborted
   ├─ Trigger: Validation fails (conflicts detected)
   ├─ Actions: Return error with conflict info
   └─ Result: Caller can retry

Invariants:
═══════════

• Only Active transactions can read/write
• Validating/Committed/Aborted are terminal states
• Once Committed, changes are durable (in WAL + storage)
• Once Aborted, changes are discarded (no effect)
• Validation is atomic (all-or-nothing)
```

---

## 7. Concurrency Comparison: M1 vs M2

```
┌─────────────────────────────────────────────────────────────┐
│         Concurrency Model: M1 (RwLock) vs M2 (OCC)          │
└─────────────────────────────────────────────────────────────┘

M1: RwLock (Pessimistic Locking)
═══════════════════════════════

Timeline:
────────────────────────────────────────────────────►

Thread 1 (Write):
┌────────────────────────────────────────────┐
│ Lock storage (write)                       │ ← Blocks everyone
│   ├─ Modify BTreeMap                       │
│   ├─ Write to WAL                          │
│   └─ Release lock                          │
└────────────────────────────────────────────┘
          ▲                         ▲
          │                         │
          Blocks                    Blocks
          │                         │
          ▼                         ▼
Thread 2 (Read):                Thread 3 (Read):
  WAIT... (blocked)                WAIT... (blocked)

Problem:
  • Writers block readers
  • Writers block writers
  • Low concurrency (serialized writes)

M2: OCC (Optimistic Concurrency Control)
═══════════════════════════════════════

Timeline:
────────────────────────────────────────────────────►

Thread 1 (Txn 1):
┌───────────────┐         ┌──────────────────────┐
│ BEGIN         │         │ COMMIT (validate)    │
│ snapshot v100 │         │   ├─ Lock (brief)    │ ← Short lock
├───────────────┤         │   ├─ Apply writes    │
│ READ (snap)   │         │   └─ Release         │
│ WRITE (buffer)│         └──────────────────────┘
└───────────────┘

Thread 2 (Txn 2):               No blocking during reads/writes
┌───────────────┐         ┌──────────────────────┐
│ BEGIN         │         │ COMMIT (validate)    │
│ snapshot v100 │         │   ├─ Lock (brief)    │
├───────────────┤         │   ├─ Check conflicts │
│ READ (snap)   │         │   └─ Abort/Commit    │
│ WRITE (buffer)│         └──────────────────────┘
└───────────────┘

Thread 3 (Txn 3):               Reads never block
┌───────────────┐
│ BEGIN         │
│ snapshot v101 │
├───────────────┤
│ READ (snap)   │ ← Concurrent with T1, T2
│ READ (snap)   │
└───────────────┘

Benefits:
  • Reads NEVER block (snapshot isolation)
  • Writes NEVER block (buffered until commit)
  • Lock held only during brief commit phase
  • High concurrency (parallel execution)

Trade-off:
  • Conflicts detected at commit (wasted work if conflict)
  • Need retry logic
  • Memory cost (snapshot cloning in M2)
```

---

## 8. Read-Your-Writes within Transaction

```
┌─────────────────────────────────────────────────────────────┐
│              Read-Your-Writes Guarantee                     │
└─────────────────────────────────────────────────────────────┘

Transaction lifecycle:

BEGIN
  ├─ snapshot: { x: 10, y: 20 }
  ├─ write_set: {}
  └─ delete_set: {}

WRITE x = 30
  ├─ snapshot: { x: 10, y: 20 }        (unchanged)
  ├─ write_set: { x → 30 }              (buffered)
  └─ delete_set: {}

READ x
  ├─ Check write_set FIRST
  │   └─ Found: x → 30
  │   └─ Return 30 (NOT 10 from snapshot)
  │
  └─ This is "read-your-writes"

WRITE y = 25
  ├─ write_set: { x → 30, y → 25 }
  └─ delete_set: {}

DELETE y
  ├─ write_set: { x → 30 }              (y removed)
  └─ delete_set: { y }                  (y deleted)

READ y
  ├─ Check write_set: not found
  ├─ Check delete_set: found!
  └─ Return None (deleted)

READ z
  ├─ Check write_set: not found
  ├─ Check delete_set: not found
  ├─ Check snapshot: z → 40
  └─ Return 40

Lookup Order (get):
═══════════════════

1. write_set:   Has key? Return value (read uncommitted writes)
2. delete_set:  Has key? Return None (read uncommitted deletes)
3. snapshot:    Has key? Return value (consistent snapshot)
4. Not found:   Return None

This ensures:
  • Transactions see their own uncommitted changes
  • External changes invisible (snapshot isolation)
  • Consistent view throughout transaction
```

---

## 9. Layer Dependencies (M2 Updated)

```
┌─────────────────────────────────────────────────────────────┐
│                  Dependency Graph (M2)                      │
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
                      └────┬────────┬───┴───┐
                           │        │       │
              depends on   │        │       │  depends on (NEW)
                           │        │       │
             ┌─────────────▼──┐  ┌──▼───────▼──────┐  ┌──────────────┐
             │   Storage      │  │   Durability    │  │ Concurrency  │ ← NEW M2
             │  (unified.rs)  │  │    (wal.rs)     │  │ (transaction)│
             │   (index.rs)   │  │  (recovery.rs)  │  │  (snapshot)  │
             └────────┬───────┘  └────┬────────────┘  └──────┬───────┘
                      │               │                      │
         depends on   │               │  depends on          │ depends on
                      │               │                      │
                      └───────┬───────┴──────────────────────┘
                              │
                              ▼
                      ┌───────────────┐
                      │  Core Types   │
                      │  (types.rs)   │
                      │  (value.rs)   │
                      │  (error.rs)   │
                      │  (traits.rs)  │
                      └───────────────┘

New in M2:
═════════

crates/concurrency/
  ├─ transaction.rs    (depends on core, storage)
  ├─ snapshot.rs       (depends on core, storage)
  ├─ validation.rs     (depends on core, storage)
  └─ cas.rs            (depends on core)

crates/engine/
  └─ database.rs       (NEW: depends on concurrency)

crates/storage/
  └─ unified.rs        (NEW: clone_data_at_version method)

Rules (unchanged):
═════════════════

✓ Allowed:    Child → Parent (downward dependencies)
✗ Forbidden:  Parent → Child (upward dependencies)
✗ Forbidden:  Sibling → Sibling at same level

New M2 flows:
════════════

✓ engine/database.rs  → concurrency/transaction.rs  (OK)
✓ concurrency/txn.rs  → storage/unified.rs          (OK)
✓ concurrency/txn.rs  → core/traits.rs              (OK)
✗ storage/unified.rs  → concurrency/txn.rs          (FORBIDDEN)
✗ primitives/kv.rs    → concurrency/txn.rs          (FORBIDDEN)
```

---

## 10. API Evolution: M1 → M2

```
┌─────────────────────────────────────────────────────────────┐
│                API Comparison: M1 vs M2                     │
└─────────────────────────────────────────────────────────────┘

M1 API (Implicit Single-Operation Transactions):
═══════════════════════════════════════════════

// Write
let version = db.put(run_id, b"key", value)?;
  ├─ Internally: begin_txn → write → commit
  ├─ Atomic: single operation
  └─ Returns: version number

// Read
let value = db.get(run_id, b"key")?;
  ├─ Direct read from storage
  ├─ No transaction needed
  └─ Returns: Option<Value>

// Delete
let old = db.delete(run_id, b"key")?;
  ├─ Internally: begin_txn → delete → commit
  ├─ Atomic: single operation
  └─ Returns: Option<Value>

M2 API (Explicit Multi-Operation Transactions):
═══════════════════════════════════════════════

// BEGIN transaction
let mut txn = db.begin_transaction(run_id)?;
  ├─ Creates snapshot at current version
  ├─ Allocates txn_id
  └─ Returns: TransactionContext

// READ (from snapshot, tracked in read_set)
let value = txn.get(&key)?;
  ├─ Reads from snapshot (or write_set if read-your-writes)
  ├─ Tracks in read_set for validation
  └─ Returns: Option<Value>

// WRITE (buffered in write_set)
txn.put(key, value)?;
  ├─ Buffers in write_set (not applied yet)
  ├─ Not visible to other transactions
  └─ Returns: ()

// COMPARE-AND-SWAP
txn.cas(key, expected_version, new_value)?;
  ├─ Buffers in cas_set
  ├─ Validation checks version at commit
  └─ Returns: ()

// DELETE (buffered in delete_set)
txn.delete(key)?;
  ├─ Buffers in delete_set
  └─ Returns: ()

// COMMIT (validate + apply)
txn.commit(&db.storage, &mut db.wal.lock())?;
  ├─ Validation: check for conflicts
  ├─ Apply: write to WAL + storage (atomic)
  ├─ Returns: Ok(()) or Err(WriteConflict)
  └─ On conflict: caller should retry

Backwards Compatibility:
════════════════════════

M1 API still works in M2:
  db.put() → internally uses begin_transaction() + commit()
  db.get() → direct read (no transaction overhead)
  db.delete() → internally uses begin_transaction() + commit()

Migration path:
  1. Keep using M1 API (no changes needed)
  2. Opt-in to M2 API for multi-operation atomicity
  3. Add retry logic for conflicts
```

---

These diagrams illustrate the key architectural components and flows for M2's Optimistic Concurrency Control implementation. They build upon M1's foundation while adding transaction management, snapshot isolation, and conflict detection.

