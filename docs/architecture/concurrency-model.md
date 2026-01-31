# Concurrency Model

StrataDB uses Optimistic Concurrency Control (OCC) with snapshot isolation. This document describes the implementation.

## OCC Lifecycle

```
1. BEGIN
   ├── Allocate transaction ID
   ├── Record snapshot version (current global version)
   └── Initialize empty read-set and write-set

2. EXECUTE
   ├── Reads: check write-set first, then storage at snapshot version
   │          Add key to read-set
   ├── Writes: buffer in write-set (not yet visible to others)
   └── CAS: validate expected version, buffer in write-set

3. VALIDATE (at commit time)
   ├── For each key in read-set:
   │     Check if current version > snapshot version
   │     If yes → CONFLICT (another transaction modified this key)
   ├── For each CAS in write-set:
   │     Verify expected version still matches
   └── If all checks pass → COMMIT

4. COMMIT
   ├── Write all changes to WAL
   ├── Apply all changes to storage atomically
   ├── Advance global version counter
   └── Return committed version number

   OR

4. ABORT
   ├── Discard write-set
   ├── Release transaction context
   └── Return TransactionConflict error
```

## Conflict Detection

### Read-Write Conflicts

If transaction T1 reads key K, and concurrent transaction T2 writes K and commits before T1, then T1 will conflict at commit time.

```
T1: begin → read(K) → ... → commit → CONFLICT
T2:            begin → write(K) → commit ✓
```

### Write-Write Conflicts

If both T1 and T2 write key K, the first to commit wins:

```
T1: begin → write(K) → commit ✓ (first committer)
T2: begin → write(K) → commit → CONFLICT
```

### Blind Writes

A "blind write" (writing without reading first) does NOT add the key to the read-set. Two transactions that blind-write the same key will not conflict — the last committer wins:

```
T1: begin → write(K, "a") → commit ✓
T2: begin → write(K, "b") → commit ✓ (overwrites T1)
```

This is intentional — blind writes are useful for append-only patterns.

### CAS Operations

CAS operations record the expected version. At validation time, the actual version is checked:

```
T1: begin → cas(K, v=1, "new") → commit ✓
T2: begin → cas(K, v=1, "new") → commit → CONFLICT (version is now 2)
```

## Per-Branch Isolation

Transactions operate within a single branch. Two transactions on different branches can never conflict because they read and write different keys (keys are prefixed with branch ID).

## Thread Safety

The concurrency layer uses:

- **DashMap** — for the main storage (lock-free concurrent hash map)
- **parking_lot** — for transaction coordination mutexes
- **Atomic counters** — for version allocation

Multiple threads can execute transactions concurrently. Conflicts are detected at commit time, not at execution time, so no locks are held during transaction execution.

## Version Allocation

Versions are allocated from a monotonically increasing atomic counter. Each committed transaction increments the counter. This provides:

- **Total ordering** of commits
- **Snapshot consistency** — a snapshot at version N sees all changes from commits <= N
- **No gaps** — versions are consecutive (modulo aborted transactions, which don't consume version numbers)

## Write-Skew

StrataDB's OCC intentionally **allows write-skew**. This is a design decision: in a write-skew scenario, two transactions read overlapping data and write to disjoint keys. Both can commit successfully because neither modified a key the other read.

This is acceptable for AI agent workloads where strict serializability is not required.

## Session Integration

The `Session` struct (in strata-executor) manages the transaction lifecycle:

1. `TxnBegin` → creates `TransactionContext` from `Database`
2. Data commands → routed through `Transaction::new(ctx, namespace)`
3. `TxnCommit` → calls `Database::commit_transaction(ctx)`
4. `TxnRollback` or `Drop` → calls `Database::end_transaction(ctx)`

