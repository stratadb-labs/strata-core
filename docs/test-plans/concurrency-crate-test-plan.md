# strata-concurrency Integration Test Plan

## Crate Overview

**strata-concurrency** implements Optimistic Concurrency Control (OCC) with snapshot isolation for transactional access to the database. Key components:

### Core Components

1. **TransactionContext** (`transaction.rs`)
   - Tracks read_set, write_set, delete_set, cas_set
   - State machine: Active → Validating → Committed/Aborted
   - Read-your-writes semantics
   - JSON path tracking for M5 region-based conflicts

2. **TransactionManager** (`manager.rs`)
   - Coordinates atomic commits
   - Per-run commit locks (prevents TOCTOU race)
   - Global version counter (monotonic)
   - Transaction ID allocator

3. **Validation** (`validation.rs`)
   - Read-set validation (first-committer-wins)
   - CAS validation (version matching)
   - JSON document/path validation
   - Per-spec Section 3 of M2_TRANSACTION_SEMANTICS

4. **Conflict Detection** (`conflict.rs`)
   - JSON path overlap detection
   - Read-write, write-write conflicts
   - Version mismatch detection

5. **Snapshot Isolation** (`snapshot.rs`)
   - ClonedSnapshotView for point-in-time consistency
   - Repeatable reads guarantee

6. **Recovery** (`recovery.rs`)
   - WAL replay for crash recovery
   - Committed transaction restoration
   - Incomplete transaction discard

## Unit Test Status (83 tests, all passing)

| Module | Tests | Quality |
|--------|-------|---------|
| conflict.rs | 20 | Excellent - all conflict types, edge cases |
| manager.rs | 12 | Good - parallel commits, stress tests |
| recovery.rs | 39 | Excellent - 11 crash scenarios, determinism |
| snapshot.rs | 25 | Good - thread safety, independence |
| wal_writer.rs | 8 | Good - lifecycle coverage |
| validation.rs | 0 | Tested via manager (acceptable) |

## Integration Test Plan

### 1. OCC Invariants (`occ_invariants.rs`)

**First-Committer-Wins Rule:**
- Two transactions read same key, both modify, first commit wins
- Second transaction gets ReadWriteConflict
- Verify conflict contains correct version information

**Blind Writes Don't Conflict:**
- Transaction writes key without reading it
- Concurrent transaction modifies same key
- Blind write should succeed (per spec Section 3.2)

**Read-Only Transactions Always Commit:**
- Transaction only reads, never writes
- Concurrent modifications to read keys
- Read-only transaction commits successfully

**Write Skew Allowed:**
- Classic write skew scenario (two accounts, constraint)
- Both transactions should commit (per spec - we don't prevent write skew)

### 2. Transaction State Machine (`transaction_states.rs`)

**Valid State Transitions:**
- Active → Validating → Committed
- Active → Validating → Aborted
- Active → Aborted (explicit abort)

**Invalid State Transitions:**
- Commit while already committed → error
- Commit while aborted → error
- Operations after commit → error
- Operations after abort → error
- Double mark_validating → error

**State Inspection:**
- is_active(), is_committed(), is_aborted()
- Status enum variants

### 3. Conflict Detection (`conflict_detection.rs`)

**Read-Write Conflicts:**
- Read key at version V, concurrent write bumps to V+1
- Validation detects mismatch

**CAS Conflicts:**
- CAS with expected_version=V, current is V+1
- CAS with expected_version=0 (key must not exist), but key exists

**JSON Document Conflicts:**
- Read JSON doc, concurrent modification
- Document-level version mismatch

**JSON Path Conflicts:**
- Read at path "a.b", write at path "a" (ancestor conflict)
- Read at path "a", write at path "a.b.c" (descendant conflict)
- Write-write overlap within same transaction

**No Conflict Cases:**
- Disjoint paths in same document
- Same path in different documents
- Blind writes

### 4. Snapshot Isolation (`snapshot_isolation.rs`)

**Point-in-Time Consistency:**
- Snapshot captures state at creation time
- Concurrent writes don't affect snapshot reads

**Repeatable Reads:**
- Multiple reads of same key return same value
- Even if underlying store changes

**Read-Your-Writes:**
- Write in transaction, read sees uncommitted write
- Delete in transaction, read sees deletion
- Write then delete, read sees deletion

**Scan Consistency:**
- Prefix scan sees consistent snapshot
- New keys added after snapshot not visible

### 5. Concurrent Transactions (`concurrent_transactions.rs`)

**Parallel Commits Different Runs:**
- Multiple transactions on different runs
- All commit in parallel (no serialization)

**Serial Commits Same Run:**
- Multiple transactions on same run
- Per-run lock ensures serialization

**High Contention Single Key:**
- Many transactions read-modify-write same key
- Only one commits, others abort with conflict

**Interleaved Operations:**
- T1 reads A, T2 reads B, T1 writes B, T2 writes A
- Both should commit (no conflict - disjoint read/write sets)

### 6. CAS Operations (`cas_operations.rs`)

**Successful CAS:**
- Read current version, CAS with correct expected_version
- Value updated atomically

**Failed CAS - Version Mismatch:**
- CAS with stale expected_version
- Transaction aborts with CASConflict

**CAS Create (expected_version=0):**
- Key doesn't exist, CAS creates it
- Key exists, CAS fails

**CAS Not In Read Set:**
- CAS is validated separately from read_set
- CAS doesn't add to read_set (per spec Section 3.4)

### 7. Transaction Lifecycle (`transaction_lifecycle.rs`)

**Begin-Commit Cycle:**
- Begin transaction, perform operations, commit
- All writes visible after commit

**Begin-Abort Cycle:**
- Begin transaction, perform operations, abort
- No writes visible (rollback)

**Commit Failure Rollback:**
- Transaction operations, validation fails
- Automatic rollback, no partial writes

**Transaction Reuse (Pooling):**
- reset() clears state but preserves capacity
- Reused transaction works correctly

### 8. Version Counter (`version_counter.rs`)

**Monotonic Increment:**
- Each commit gets unique, increasing version
- No gaps in version sequence

**Concurrent Uniqueness:**
- Many threads allocating versions simultaneously
- All versions unique

**Wrap-Around Handling:**
- Version counter at u64::MAX - 1
- Wraps to 0 correctly

**Recovery Restoration:**
- After crash, version counter restored from WAL
- New versions continue from correct point

### 9. Stress Tests (`stress.rs`) - All `#[ignore]`

**High Concurrency Read-Write:**
- 8+ threads, mix of reads and writes
- Verify no data corruption

**Rapid Transaction Throughput:**
- Maximum transactions per second
- Measure commit latency

**Large Transaction Sets:**
- Transaction with 10K+ operations
- Memory and performance acceptable

**Long-Running Transactions:**
- Transaction held open while others commit
- Eventually commits or properly conflicts

## Test Infrastructure

Tests will use:
- `tests/common/mod.rs` for shared utilities
- `Database::ephemeral()` for fast in-memory testing
- `tempfile` for persistent tests when needed
- Standard `#[test]` with `#[ignore]` for stress tests

## File Structure

```
tests/concurrency/
├── main.rs
├── occ_invariants.rs
├── transaction_states.rs
├── conflict_detection.rs
├── snapshot_isolation.rs
├── concurrent_transactions.rs
├── cas_operations.rs
├── transaction_lifecycle.rs
├── version_counter.rs
└── stress.rs
```

## Verification

After implementation:
```bash
cargo test --test concurrency
```

Expected: ~80-100 tests, all passing (stress tests ignored by default).
