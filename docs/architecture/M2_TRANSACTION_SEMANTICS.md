# M2 Transaction Semantics Specification

**Version**: 1.0
**Status**: CRITICAL - Blocks All M2 Implementation
**Story**: #78
**Last Updated**: 2026-01-11

---

## Purpose

This document defines the **semantic contract** for all M2 transaction behavior. Every M2 implementation MUST follow these rules exactly. This specification is the **authoritative reference** for:

- What isolation guarantees we provide
- What a transaction can and cannot see
- When transactions conflict
- How implicit (M1-style) operations behave
- How replay reconstructs state
- How versions are assigned and used

**Read this document before writing any M2 code.**

---

## Core Invariants

These invariants are the **safety checklist** for all M2 implementation. Every design decision and code path must preserve these guarantees:

| Invariant | Description |
|-----------|-------------|
| **No partial commits** | A transaction never observes another transaction's partial writes. Either all writes are visible or none are. |
| **All-or-nothing commit** | A transaction's writes either all succeed (commit) or all fail (abort). No partial application. |
| **Monotonic versions** | Version numbers never decrease. Global version and key versions always increase. |
| **Deterministic replay** | Given the same WAL, replay always produces identical state regardless of when or where it runs. |
| **Non-blocking reads** | No transaction blocks another transaction's reads. Readers never wait for writers. |
| **Non-blocking writes** | No transaction blocks another transaction's writes. Writers never wait (conflict detected at commit). |
| **Read-your-writes** | A transaction always sees its own uncommitted modifications. |
| **Snapshot consistency** | All reads within a transaction see data from a single consistent point in time. |

If any code path violates these invariants, it is a bug.

---

## Table of Contents

0. [Core Invariants](#core-invariants)
1. [Isolation Level Declaration](#1-isolation-level-snapshot-isolation)
2. [Visibility Rules](#2-visibility-rules)
3. [Conflict Detection](#3-conflict-detection)
4. [Implicit Transactions](#4-implicit-transactions)
5. [Replay Semantics](#5-replay-semantics)
6. [Version Semantics](#6-version-semantics)
7. [Appendix: Design Decisions](#appendix-design-decisions)

---

## 1. Isolation Level: Snapshot Isolation

**We implement Snapshot Isolation (SI), NOT Serializability.**

This is a deliberate design choice. Snapshot Isolation:

| Guarantee | Provided? | Description |
|-----------|-----------|-------------|
| No dirty reads | ✅ YES | Never see uncommitted data from other transactions |
| No non-repeatable reads | ✅ YES | Same key returns same value within a transaction |
| No lost updates | ✅ YES | CAS and read-validation prevent silent overwrites |
| No write skew | ❌ NO | Two transactions can modify different keys based on same read |
| No phantom reads | ❌ NO | New keys may appear in range scans from concurrent commits |
| Serializable | ❌ NO | Execution order may not correspond to any serial order |

**We explicitly do NOT guarantee serializability.**

This means some anomalies are ALLOWED and are intended behavior:

- **Write skew**: T1 reads A, writes B; T2 reads B, writes A - both commit
- **Phantom reads**: New keys appearing in range scans after concurrent commits

**Do NOT attempt to "fix" these behaviors - they are by design.**

### 1.1 Why Snapshot Isolation?

Snapshot Isolation is the chosen isolation level because:

1. **Simpler implementation**: No predicate locking required
2. **Better performance**: Readers never block writers, writers never block readers
3. **Acceptable for agent workloads**: Write skew is rare in practice for agent state
4. **Industry proven**: PostgreSQL, Oracle, and SQL Server offer SI as a valid isolation level

### 1.2 What This Means for Developers

When writing transaction code:

- **DO** use CAS operations when you need atomic read-modify-write
- **DO** read all keys you depend on before writing (adds them to read-set)
- **DO NOT** assume transactions execute in any particular serial order
- **DO NOT** add extra locking to "fix" write skew unless explicitly required

**⚠️ Write Skew Prevention Rule**: If your logic depends on a multi-key invariant (e.g., "balance_a + balance_b >= 100"), you MUST read ALL keys involved in that invariant before writing ANY of them. This adds all keys to your read-set, causing a conflict if any are modified concurrently.

---

## 2. Visibility Rules

This section exhaustively defines what data a transaction can observe.

### 2.1 What a Transaction ALWAYS Sees

| Data | Visibility | Mechanism |
|------|------------|-----------|
| Committed data as of `start_version` | ✅ Always visible | Snapshot captures state at begin |
| Its own uncommitted writes | ✅ Always visible | Read-your-writes from write_set |
| Its own uncommitted deletes | ✅ Always visible | Key returns None from delete_set |

**Rule**: A transaction sees a consistent snapshot of committed data from when it began, plus all its own pending modifications.

### 2.2 What a Transaction NEVER Sees

| Data | Visibility | Reason |
|------|------------|--------|
| Uncommitted writes from other transactions | ❌ Never visible | Isolation guarantee |
| Writes committed AFTER `start_version` | ❌ Never visible | Snapshot boundary |
| Partial writes from any transaction | ❌ Never visible | Atomicity guarantee |
| Rolled-back writes | ❌ Never visible | Never applied to storage |

**Rule**: A transaction is completely isolated from concurrent activity until commit time.

### 2.3 What a Transaction MAY See (Anomalies)

These are allowed anomalies under Snapshot Isolation:

| Anomaly | Description | Why Allowed |
|---------|-------------|-------------|
| Phantom reads | Range scan returns different keys after concurrent commit | SI does not track predicates |
| Write skew results | Final state violates constraint that each txn individually satisfied | SI validates keys, not predicates |

**These are intended behaviors, not bugs.**

### 2.4 Visibility Examples

#### Example 1: Snapshot Isolation (Concurrent Commit Invisible)

```
Initial state: key_a = "old_value" (version 100)

Timeline:
  T1: BEGIN (start_version=100)
  T2: BEGIN (start_version=100)
  T2: PUT(key_a, "new_value")
  T2: COMMIT → SUCCESS (key_a now version 101)
  T1: GET(key_a) → Returns "old_value" (from T1's snapshot at version 100)
  T1: PUT(key_b, "some_value")
  T1: COMMIT → SUCCESS (T1 never read key_a, so no conflict)
```

**Key insight**: T1 does not see T2's committed write because T1's snapshot predates T2's commit.

#### Example 2: Read-Your-Writes

```
T1: BEGIN
T1: GET(key_a) → Returns "original" (from snapshot)
T1: PUT(key_a, "modified")
T1: GET(key_a) → Returns "modified" (from T1's write_set, not snapshot)
T1: DELETE(key_a)
T1: GET(key_a) → Returns None (from T1's delete_set)
T1: COMMIT
```

**Key insight**: Within a transaction, you always see your own pending changes.

#### Example 3: Write Skew (ALLOWED ANOMALY)

```
Initial state:
  balance_a = 100 (version 50)
  balance_b = 100 (version 51)
  Constraint: balance_a + balance_b >= 100

Timeline:
  T1: BEGIN (start_version=51)
  T2: BEGIN (start_version=51)
  T1: READ(balance_a) → 100
  T1: CHECK: 100 + 100 >= 100? YES
  T2: READ(balance_b) → 100
  T2: CHECK: 100 + 100 >= 100? YES
  T1: WRITE(balance_b = 0)
  T2: WRITE(balance_a = 0)
  T1: COMMIT → SUCCESS
      (balance_a not in T1's read_set, balance_b written by T1)
  T2: COMMIT → SUCCESS
      (balance_b not in T2's read_set, balance_a written by T2)

Final state:
  balance_a = 0
  balance_b = 0
  CONSTRAINT VIOLATED: 0 + 0 = 0 < 100
```

**This is intended behavior under Snapshot Isolation.**

Why both transactions succeed:
- T1 only read `balance_a`, only wrote `balance_b` → no conflict
- T2 only read `balance_b`, only wrote `balance_a` → no conflict
- Neither transaction saw the other's write

**Do NOT try to prevent this - it is by design.** If you need to prevent write skew, read ALL keys involved in your constraint before writing any of them.

#### Example 4: Phantom Read (ALLOWED ANOMALY)

```
Initial state: Keys = {user:1, user:2}

Timeline:
  T1: BEGIN (start_version=100)
  T1: SCAN(prefix="user:") → Returns [user:1, user:2]
  T2: BEGIN (start_version=100)
  T2: PUT(user:3, "new_user")
  T2: COMMIT → SUCCESS (version 101)
  T1: SCAN(prefix="user:") → Still returns [user:1, user:2] (snapshot)
  T1: COMMIT → SUCCESS

After T1 commits:
  T3: BEGIN (start_version=101)
  T3: SCAN(prefix="user:") → Returns [user:1, user:2, user:3]
```

**Key insight**: T1's snapshot doesn't see user:3, but T3's snapshot does. This is a phantom read if you expected the same query to return the same results across transactions.

---

## 3. Conflict Detection

This section defines precisely when transactions conflict and must abort.

### 3.1 When a Transaction ABORTS

A transaction aborts at COMMIT time if ANY of these conditions are true:

#### Condition 1: Read-Write Conflict

```
Definition:
  - T1 read key K and recorded version V in its read_set
  - At commit time, the current storage version of K is V' where V' != V

Result: T1 ABORTS

Example:
  T1: BEGIN (start_version=100)
  T1: GET(key_a) → value at version 95 (records read_set[key_a] = 95)
  T2: PUT(key_a, "new_value") + COMMIT → key_a now version 101
  T1: PUT(key_b, "something")
  T1: COMMIT
      Validation: read_set contains {key_a: 95}
      Current version of key_a = 101
      95 != 101 → CONFLICT
  Result: T1 ABORTS
```

#### Condition 2: Write-Write Conflict (with prior read)

```
Definition:
  - T1 read key K (adding it to read_set)
  - T1 is also writing key K (in write_set)
  - Another transaction modified K after T1's start_version

Result: T1 ABORTS (this is a subset of Condition 1)

Example:
  T1: BEGIN (start_version=100)
  T1: GET(key_a) → records read_set[key_a] = version
  T2: PUT(key_a, "new") + COMMIT
  T1: PUT(key_a, "modified")
  T1: COMMIT → ABORT (read_set validation fails for key_a)
```

#### Condition 3: CAS Conflict

```
Definition:
  - T1 called CAS(K, expected_version=V, new_value)
  - At commit time, current storage version of K != V

Result: T1 ABORTS

Example:
  T1: BEGIN
  T1: CAS(counter, expected_version=5, new_value=10)
  T2: CAS(counter, expected_version=5, new_value=20) + COMMIT → counter now version 6
  T1: COMMIT
      Validation: CAS expected version 5, current version is 6
      5 != 6 → CONFLICT
  Result: T1 ABORTS
```

#### Condition 4: Delete Conflict

```
Definition:
  - T1 read key K at version V, then deleted K
  - At commit time, current storage version of K != V

Result: T1 ABORTS (same as read-write conflict)

Example:
  T1: BEGIN
  T1: GET(key_a) → records read_set[key_a] = 50
  T2: PUT(key_a, "updated") + COMMIT → key_a now version 51
  T1: DELETE(key_a)
  T1: COMMIT
      Validation: read_set[key_a] = 50, current = 51
      50 != 51 → CONFLICT
  Result: T1 ABORTS
```

### 3.2 When a Transaction DOES NOT Conflict

These scenarios are explicitly NOT conflicts:

#### Scenario 1: Blind Write (write without read)

```
Definition:
  - T1 writes key K without ever reading it first
  - T2 also writes key K and commits first

Result: T1 COMMITS successfully (overwrites T2's value)

Example:
  T1: BEGIN
  T1: PUT(key_a, "value_from_T1")  // No GET(key_a) first - blind write
  T2: BEGIN
  T2: PUT(key_a, "value_from_T2")  // Also blind write
  T2: COMMIT → SUCCESS (key_a = "value_from_T2")
  T1: COMMIT → SUCCESS (key_a = "value_from_T1", overwrites T2)

Why no conflict: Neither transaction read key_a, so neither has it in their read_set. Write-write conflict only applies when the key was also read.
```

**Important**: First-committer-wins is based on the READ-SET, not the write-set.

#### Scenario 2: Different Keys

```
Definition:
  - T1 reads/writes key A
  - T2 reads/writes key B
  - A and B are different keys

Result: Both COMMIT (no conflict)

Example:
  T1: BEGIN
  T1: GET(key_a), PUT(key_a, "new")
  T2: BEGIN
  T2: GET(key_b), PUT(key_b, "new")
  T1: COMMIT → SUCCESS
  T2: COMMIT → SUCCESS
```

#### Scenario 3: Read-Only Transaction

```
Definition:
  - T1 only reads keys, never writes any

Result: ALWAYS COMMITS

Example:
  T1: BEGIN
  T1: GET(key_a) → "value_a"
  T1: GET(key_b) → "value_b"
  T2: PUT(key_a, "modified") + COMMIT
  T1: COMMIT → SUCCESS (read-only transactions always succeed)
```

**Why**: Read-only transactions have no writes to validate. They simply return their snapshot view.

### 3.3 First-Committer-Wins Explained

"First committer wins" means:
1. The first transaction to COMMIT gets its writes applied
2. Later transactions that CONFLICT with those writes must abort
3. **Conflict is based on READ-SET, not write-set**

#### Example: Both Read Same Key

```
Initial: key_x = "initial" (version 100)

T1: BEGIN, GET(key_x), PUT(key_x, "from_T1")
    read_set = {key_x: 100}
    write_set = {key_x: "from_T1"}

T2: BEGIN, GET(key_x), PUT(key_x, "from_T2")
    read_set = {key_x: 100}
    write_set = {key_x: "from_T2"}

T1: COMMIT → SUCCESS
    Validation: key_x current version = 100, read version = 100 ✓
    Apply: key_x = "from_T1", version = 101

T2: COMMIT → ABORT
    Validation: key_x current version = 101, read version = 100 ✗
    Conflict detected
```

**Result**: T1 wins (first to commit), T2 must abort and retry.

#### Example: Neither Read Same Key (Blind Writes)

```
T1: BEGIN, PUT(key_x, "from_T1")  // No read
    read_set = {}
    write_set = {key_x: "from_T1"}

T2: BEGIN, PUT(key_x, "from_T2")  // No read
    read_set = {}
    write_set = {key_x: "from_T2"}

T1: COMMIT → SUCCESS
    Validation: read_set is empty, nothing to validate
    Apply: key_x = "from_T1"

T2: COMMIT → SUCCESS
    Validation: read_set is empty, nothing to validate
    Apply: key_x = "from_T2" (overwrites T1)
```

**Result**: Both succeed. T2 overwrites T1. This is intended behavior for blind writes.

### 3.4 CAS Interaction with Read/Write Validation

CAS operations are validated SEPARATELY from the read-set:

| Operation | Read-Set Entry? | CAS Validation |
|-----------|-----------------|----------------|
| `txn.get(key)` | ✅ Yes, adds to read_set | N/A |
| `txn.cas(key, version, value)` | ❌ No, does NOT add to read_set | ✅ Checks expected_version |
| `txn.get(key)` then `txn.cas(key, ...)` | ✅ Yes, from the get() | ✅ Both checks apply |

#### CAS Without Read - Only Version Check

```rust
// CAS alone does NOT add to read_set
txn.cas(counter_key, expected_version=5, new_value=10)?;

// At commit:
// - CAS validation: current version of counter_key must equal 5
// - Read-set validation: counter_key NOT in read_set, no check
```

If you want both CAS and read-set protection:

```rust
// Explicit read adds to read_set
let current = txn.get(&counter_key)?;  // Adds to read_set
txn.cas(counter_key, current.version, new_value)?;

// At commit:
// - CAS validation: version must equal current.version
// - Read-set validation: version must not have changed since read
// (These are redundant in this case, but both are checked)
```

#### Why CAS Doesn't Auto-Add to Read-Set

CAS is a conditional write operation. The expected_version is the condition. If you want read-set protection, explicitly read first. This gives developers control over conflict detection granularity.

---

## 4. Implicit Transactions

M1-style operations (`db.put()`, `db.get()`) continue to work in M2. Each operation is wrapped in an implicit single-operation transaction.

### 4.1 What is an Implicit Transaction?

An implicit transaction is:
- Automatically created for single M1-style operations
- Contains exactly one read or write
- Commits immediately after the operation
- Invisible to the caller (behaves like M1)

### 4.2 Implicit Transaction Behavior

#### `db.put(key, value)` Behavior

```rust
// User calls:
db.put(run_id, key, value)?;

// Internally executes as:
{
    let mut txn = db.begin_transaction(run_id)?;
    txn.put(key.clone(), value)?;
    txn.commit()?;
}
```

Properties:
- **IS atomic**: All-or-nothing (the single put either commits or doesn't)
- **CAN conflict**: If another transaction modifies the same key between begin and commit
- **In practice**: Very short window between begin and commit, conflicts are rare

#### `db.get(key)` Behavior

```rust
// User calls:
let value = db.get(run_id, key)?;

// Internally executes as:
{
    let txn = db.begin_transaction(run_id)?;
    let result = txn.get(&key)?;
    txn.commit()?;  // Read-only, always succeeds
    result
}
```

Properties:
- **Creates a snapshot**: At the current version
- **Returns consistent value**: Point-in-time read
- **Always commits**: Read-only transactions never conflict

#### `db.delete(key)` Behavior

```rust
// User calls:
db.delete(run_id, key)?;

// Internally executes as:
{
    let mut txn = db.begin_transaction(run_id)?;
    txn.delete(key.clone())?;
    txn.commit()?;
}
```

Properties:
- Same as `db.put()` - single operation, atomic, can conflict (rarely)

### 4.3 Can Implicit Transactions Conflict?

**Yes**, but rarely in practice due to the very short transaction duration.

```
Thread 1: db.put(key, "A")  // BEGIN → PUT → COMMIT
Thread 2: db.put(key, "B")  // BEGIN → PUT → COMMIT

Possible outcomes:
1. T1 commits, then T2 commits → key = "B"
2. T2 commits, then T1 commits → key = "A"
3. T1 and T2 overlap during commit:
   - If neither read the key first: both succeed, last write wins
   - If using internal read-modify-write: one may retry
```

Implicit transactions use the same retry logic as explicit transactions. The `db.put()` implementation includes automatic retry on conflict.

### 4.4 Mixing Implicit and Explicit Transactions

#### Safe Pattern: Sequential Operations

```rust
// This is safe:
db.put(key_a, "value")?;  // Implicit txn #1, commits immediately

db.transaction(|txn| {
    let v = txn.get(&key_a)?;  // Sees committed value from implicit txn #1
    txn.put(key_b, v)?;
    Ok(())
})?;  // Explicit txn #2, commits here
```

The explicit transaction sees the committed result of the implicit transaction because they execute sequentially.

#### Dangerous Pattern: Nested Implicit Inside Explicit

```rust
// This is NOT recommended:
db.transaction(|txn| {
    txn.put(key_a, "in_txn")?;

    db.put(key_b, "implicit")?;  // DIFFERENT transaction! Commits immediately!
    // At this point:
    // - key_b is now visible to OTHER transactions
    // - key_a is NOT visible (still in txn's write_set)

    Ok(())
})?;
```

**Why this is dangerous**:
1. `db.put(key_b, ...)` creates a NEW transaction, separate from `txn`
2. The implicit transaction commits immediately
3. Other transactions can now see `key_b` but NOT `key_a`
4. If the explicit transaction aborts, `key_b` is already committed
5. Atomicity is broken - `key_a` and `key_b` are NOT atomic together

**Rule**: Do not mix implicit and explicit transactions for related data. Use explicit transactions for all related operations.

### 4.5 When to Use Implicit vs Explicit

| Use Case | Recommendation |
|----------|----------------|
| Single key read | Implicit (`db.get()`) |
| Single key write | Implicit (`db.put()`) |
| Read-modify-write single key | Explicit with CAS |
| Multiple related keys | Explicit transaction |
| Atomic batch operations | Explicit transaction |
| Conditional updates | Explicit with CAS |

---

## 5. Replay Semantics

Replay reconstructs database state by re-applying WAL entries. This section defines how replay behaves.

### 5.1 What is Replay?

Replay is used for:
- **Crash recovery**: Restore state after unexpected shutdown
- **Point-in-time recovery**: Reconstruct state at a specific version
- **Debugging/auditing**: Trace how state evolved

### 5.2 Replay Rules

#### Rule 1: Replays Do NOT Re-Run Conflict Detection

```
WAL contains only COMMITTED transactions.
If a transaction is in the WAL, it already passed validation at commit time.
Replay applies writes directly without re-validating.
```

Rationale: The WAL is a record of what DID happen, not what MIGHT happen. Conflict detection was already performed; replaying it would be redundant and potentially incorrect (state has changed).

#### Rule 2: Replays Apply Commit Decisions, Not Re-Execute Logic

```
WAL entry: Write { key: "counter", value: 42, version: 10 }

Replay action: storage.put("counter", 42, version=10)

We do NOT:
- Re-read the old value
- Re-compute new_value = old_value + 1
- Re-run the transaction closure
```

The WAL contains the **result** of transaction logic, not the logic itself. Replay applies results directly.

#### Rule 3: Replays Are Single-Threaded

```
WAL is a sequential log.
Replay processes entries in order: entry 1, entry 2, entry 3, ...
No concurrency during replay.
All writes are applied in WAL order.
```

Rationale: The WAL ordering reflects the actual commit order. Concurrent replay could violate this ordering.

#### Rule 4: Versions Are Preserved Exactly

```
Original commit:
  storage.put(key, value) → assigned version 42

WAL entry:
  Write { key, value, version: 42 }

Replay:
  storage.put_with_version(key, value, 42)  // Uses recorded version

After replay:
  storage.current_version() == same as before crash
```

Version numbers are part of the persistent state. Replay must preserve them exactly.

### 5.3 WAL Entry Format (M2)

```rust
enum WALEntry {
    // Transaction boundary markers
    BeginTxn {
        txn_id: u64,
        run_id: RunId,
        timestamp: u64,
    },

    // Data modifications
    Write {
        key: Key,
        value: Value,
        version: u64,
    },
    Delete {
        key: Key,
        version: u64,  // Version at which delete occurred
    },

    // Commit marker
    CommitTxn {
        txn_id: u64,
        commit_version: u64,  // Global version after this commit
    },

    // Note: No AbortTxn in M2
    // Aborted transactions write nothing to WAL
}
```

### 5.4 Recovery Algorithm

```
RECOVERY PROCEDURE:

1. Load snapshot (if exists)
   - Provides base state at snapshot_version
   - Skip WAL entries <= snapshot_version

2. Open WAL, scan for entries after snapshot_version
   - Build map: txn_id → [entries]
   - Track which txn_ids have CommitTxn markers

3. Identify incomplete transactions
   - Incomplete = has BeginTxn but no CommitTxn
   - These represent crashed-during-commit

4. For each COMPLETE transaction (has CommitTxn):
   - Apply all Write entries: storage.put(key, value, version)
   - Apply all Delete entries: storage.delete(key)
   - Update global version counter to commit_version

5. DISCARD incomplete transactions
   - Do not apply their Write/Delete entries
   - They were never committed

6. Database ready for new operations
```

### 5.5 Incomplete Transaction Handling

If a crash occurs during commit:

```
Scenario: Crash mid-commit

WAL state after crash:
  Entry 100: BeginTxn { txn_id: 42 }
  Entry 101: Write { key: "a", value: 1, version: 50 }
  Entry 102: Write { key: "b", value: 2, version: 51 }
  -- CRASH HERE (no CommitTxn for txn_id 42) --

Recovery:
  - Sees BeginTxn for txn_id 42
  - Sees Write entries for txn_id 42
  - Does NOT see CommitTxn for txn_id 42
  - Conclusion: Transaction 42 is INCOMPLETE
  - Action: DISCARD all entries for txn_id 42
  - Result: Keys "a" and "b" are NOT modified
```

**Why no AbortTxn entry in M2**:
- Aborted transactions never write anything (abort before commit)
- Recovery identifies incomplete transactions by missing CommitTxn
- Simpler WAL format
- M3+ may add explicit AbortTxn for auditing purposes

### 5.6 Replay Determinism

Given the same WAL, replay MUST produce identical state:

```
Property: DETERMINISTIC REPLAY

For any WAL W:
  replay(W) at time T1 == replay(W) at time T2

For any two systems S1, S2:
  replay(W) on S1 == replay(W) on S2
```

This is guaranteed because:
1. WAL entries include exact versions
2. Replay is single-threaded
3. No external state consulted during replay
4. Operations are pure functions on (key, value, version)

---

## 6. Version Semantics

This section defines how version numbers work in the system.

### 6.1 Global Version Counter

- **Single monotonic counter** for the entire database
- **Incremented on each COMMIT** (not each write)
- **Used for snapshot isolation**: `start_version` captures the commit point

```
State:
  global_version = 100

Transaction T1 commits with 3 writes:
  - Write key_a
  - Write key_b
  - Write key_c

After T1 commit:
  global_version = 101  // Incremented ONCE for the whole transaction
  key_a.version = 101
  key_b.version = 101
  key_c.version = 101
```

Alternative design considered: Increment per write. Rejected because it doesn't match snapshot semantics - a transaction should see a consistent point, not partial commits.

### 6.2 Key Versions

Each key has its own version number:

```rust
struct VersionedValue {
    value: Value,
    version: u64,        // Version when this value was written
    expires_at: Option<Instant>,  // TTL expiration
}
```

Key version semantics:
- **Incremented** when the key is written (set to global_version at commit)
- **Stored with the value**: version is part of persistent state
- **Used for conflict detection**: read_set records version seen

### 6.3 Snapshot Version vs Key Version

```
Global version: 100

Storage state:
  key_a: { value: "A", version: 50 }   // Written at global version 50
  key_b: { value: "B", version: 80 }   // Written at global version 80
  key_c: { value: "C", version: 100 }  // Written at global version 100

T1 begins:
  T1.start_version = 100  // Snapshot boundary

T1 reads:
  key_a → version 50 (< 100, visible) → read_set[key_a] = 50
  key_b → version 80 (< 100, visible) → read_set[key_b] = 80
  key_c → version 100 (== 100, visible) → read_set[key_c] = 100

Concurrent T2 commits:
  T2 writes key_a → key_a.version = 101
  global_version = 101

T1 reads key_a again:
  From snapshot (version 100), sees key_a at version 50
  Does NOT see version 101 (after snapshot)

T1 commits:
  Validation checks:
    read_set[key_a] = 50, current key_a.version = 101
    50 != 101 → CONFLICT
  Result: T1 ABORTS
```

### 6.4 Version 0 Semantics

Version 0 has special meaning: **the key has never existed**.

| Context | Version 0 Meaning |
|---------|-------------------|
| Key lookup returns version 0 | Key has never been created (not even as a tombstone) |
| `read_set[key] = 0` | Transaction read a truly non-existent key |
| `CAS(key, expected_version=0, value)` | Create only if key has never existed |

**Important distinction**: A deleted key (tombstone) has version > 0. Only keys that have *never* been created have version 0. See Section 6.5 for tombstone semantics.

### 6.5 Delete Semantics and Tombstones

**M2 uses tombstone-based deletion** to enable conflict detection on deleted keys:

| Aspect | Behavior |
|--------|----------|
| **Storage representation** | Deleted keys are stored as tombstones: `{ value: None, version: V, deleted: true }` |
| **Version after delete** | Tombstone gets a new version (incremented at delete time) |
| **Read behavior** | Reading a tombstoned key returns `None` |
| **Read-set tracking** | Reading a tombstone records the tombstone's version in read_set (NOT version 0) |
| **Conflict detection** | If tombstone version changes (key re-created or deleted again), reader conflicts |
| **Re-creation** | Writing to a tombstoned key replaces the tombstone with a new versioned value |

**Tombstone vs Never-Existed**:

```
Scenario: Key "x" was created (v10), then deleted (v20)

Storage state: { key: "x", value: None, version: 20, deleted: true }

T1: GET("x") → Returns None
    read_set["x"] = 20 (tombstone version, NOT 0)

T2: PUT("x", "new_value") → COMMIT
    Storage: { key: "x", value: "new_value", version: 21 }

T1: COMMIT
    Validation: read_set["x"] = 20, current version = 21
    20 != 21 → CONFLICT → ABORT
```

```
Scenario: Key "y" never existed

T1: GET("y") → Returns None
    read_set["y"] = 0 (truly non-existent)

T2: PUT("y", "created") → COMMIT
    Storage: { key: "y", value: "created", version: 50 }

T1: COMMIT
    Validation: read_set["y"] = 0, current version = 50
    0 != 50 → CONFLICT → ABORT
```

**CAS with deleted keys**:

| Operation | On never-existed key | On tombstoned key (version 20) |
|-----------|---------------------|-------------------------------|
| `CAS(key, 0, value)` | SUCCESS (creates key) | FAILS (version 20 != 0) |
| `CAS(key, 20, value)` | FAILS (version 0 != 20) | SUCCESS (re-creates key) |

**Why tombstones matter**:

1. **Conflict detection**: Without tombstones, we couldn't detect when a deleted key is re-created
2. **Snapshot consistency**: Snapshots can distinguish "never existed" from "deleted before snapshot"
3. **WAL replay**: Delete operations need version numbers for deterministic replay

**Tombstone cleanup (M3+ consideration)**:
Tombstones accumulate. Future milestones may add garbage collection when no active snapshots reference the tombstone. For M2, tombstones are retained indefinitely (acceptable for agent workloads).

#### CAS with Version 0: Insert-If-Not-Exists

```rust
// Create a key only if it doesn't exist
txn.cas(new_key, expected_version=0, initial_value)?;

// At commit:
// If new_key doesn't exist: create with initial_value → SUCCESS
// If new_key exists (any version): ABORT (version != 0)
```

#### Reading Non-Existent Keys

```rust
txn.get(&missing_key)?;  // Returns None, records read_set[missing_key] = 0

// Later, another transaction creates missing_key (version 50)

txn.commit();
// Validation: read_set[missing_key] = 0, current version = 50
// 0 != 50 → CONFLICT → ABORT
```

**Key insight**: Reading a non-existent key is still tracked. If someone creates that key before you commit, you conflict. This prevents "insert anomalies" where two transactions both think they're creating a new key.

### 6.5 Version Ordering Guarantees

```
Guarantee: VERSION MONOTONICITY

For any key K with writes at times T1 < T2:
  version(K, T1) < version(K, T2)

For the global counter:
  After commit C1 before commit C2:
  global_version after C1 < global_version after C2
```

Versions never decrease. This is essential for snapshot isolation to function.

---

## Appendix: Design Decisions

This appendix documents the rationale behind key design choices.

### A.1 Why Snapshot Isolation (Not Serializable)?

| Factor | Snapshot Isolation | Serializable |
|--------|-------------------|--------------|
| Implementation complexity | Moderate | High (predicate locking, SSI) |
| Read performance | Excellent (no locks) | Good (may need locks) |
| Write skew prevention | No | Yes |
| Deadlock risk | None (OCC) | Possible (with locks) |
| Agent workload fit | Excellent | Overkill |

**Decision**: Snapshot Isolation

**Rationale**:
1. Agents typically work on disjoint data (their own run state)
2. Write skew requires reading A, writing B; reading B, writing A - rare in practice
3. Simpler implementation reduces bugs
4. Performance is paramount for low-latency agent operations
5. If write skew prevention is needed, use CAS explicitly

### A.2 Why First-Committer-Wins?

| Approach | Behavior | Deadlock Risk |
|----------|----------|---------------|
| First-committer-wins | Abort loser, retry | None |
| First-locker-wins | Block until lock available | Yes |
| Multi-version with SSI | Complex validation | None |

**Decision**: First-committer-wins with OCC

**Rationale**:
1. No deadlocks - essential for system stability
2. Simple conflict resolution - loser retries with fresh snapshot
3. Natural fit for OCC - validate at commit, not during execution
4. Low conflict expected - retry overhead acceptable

### A.3 Why No AbortTxn WAL Entry (M2)?

| Approach | WAL Contents | Recovery |
|----------|--------------|----------|
| Explicit AbortTxn | BeginTxn, Writes, AbortTxn | Look for AbortTxn |
| No AbortTxn | BeginTxn, Writes (no CommitTxn) | Look for missing CommitTxn |

**Decision**: No AbortTxn entry in M2

**Rationale**:
1. Aborted transactions don't write to storage, so WAL entries are unnecessary
2. Recovery can identify incomplete transactions by missing CommitTxn
3. Simpler WAL format - fewer entry types to handle
4. Reduced write amplification - no WAL write on abort

**Future (M3+)**: May add AbortTxn for auditing, but not required for functionality.

### A.4 Why Clone Snapshot (Not Lazy)?

| Approach | Memory | Speed | Complexity |
|----------|--------|-------|------------|
| Clone (M2) | O(data size) | O(data size) to create | Low |
| Lazy (M3+) | O(1) | O(1) to create | Higher |

**Decision**: Clone snapshot for M2

**Rationale**:
1. Simpler implementation - reduces risk for initial transaction support
2. Acceptable for expected data sizes (< 10K keys per run)
3. Clear correctness - snapshot is immutable after creation
4. M3 will add lazy snapshots as optimization when needed

### A.5 Why CAS Doesn't Auto-Add to Read-Set?

| Approach | Behavior | Developer Control |
|----------|----------|-------------------|
| CAS auto-adds | CAS always checks read-set | Less (forced read tracking) |
| CAS independent | CAS only checks expected_version | More (explicit read if needed) |

**Decision**: CAS does not automatically add to read-set

**Rationale**:
1. CAS is a conditional write, not a read operation
2. Developers may want CAS without read-set overhead
3. Explicit `get()` + `cas()` is clear about intent
4. Follows principle of least surprise - operations do one thing

---

## Validation Checklist

This document satisfies all acceptance criteria:

- [x] Section 1: Isolation level explicitly declared as "NOT serializable"
- [x] Section 2: All visibility rules defined (ALWAYS/NEVER/MAY see)
- [x] Section 2: Write skew example included and marked as intended behavior
- [x] Section 3: All conflict conditions precisely defined
- [x] Section 3: First-committer-wins explained with examples
- [x] Section 3: CAS interaction with read-set documented
- [x] Section 4: Implicit transactions fully specified
- [x] Section 4: db.put(), db.get(), db.delete() behavior defined
- [x] Section 4: Implicit transaction conflict behavior documented
- [x] Section 5: Replay rules defined (no re-validation)
- [x] Section 5: Single-threaded replay stated
- [x] Section 5: Version preservation documented
- [x] Section 6: Version semantics (global vs key) explained
- [x] Section 6: Version 0 meaning documented
- [x] No ambiguous language ("proper", "correct", "good")
- [x] Every statement is testable

---

**Document Version**: 1.0
**Status**: Ready for Review
**Story**: #78
**Author**: Claude (Story Implementation)
**Date**: 2026-01-11
