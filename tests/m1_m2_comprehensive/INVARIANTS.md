# M1+M2 Test Invariants

This document defines the **exact invariants** that each test must enforce.
Every test should map to exactly one invariant. If it doesn't, it's noise.

---

## M1 Invariants (Storage + WAL)

M1 is: "Basic storage and WAL without transactions"

That means:
- Single-threaded correctness
- Deterministic state rebuild
- Append-only semantics
- **No** isolation
- **No** concurrency guarantees
- **No** snapshot isolation
- **No** ACID

### M1.1: WAL Append-Only Semantics
**Definition**: WAL never mutates old entries. New operations append.

### M1.2: Deterministic Replay
**Definition**: Given the same WAL, replay always produces identical state.
```
For any WAL W:
  replay(W) at time T1 == replay(W) at time T2
```

### M1.3: State Reconstruction Correctness
**Definition**: In-memory state == WAL fold. If I replay WAL from empty, final state must match original.
```
state_after_crash_recovery == state_before_crash
```

### M1.4: Crash Consistency
**Definition**: If a crash happens between WAL append and memory update, replay produces correct state.

### M1.5: No Phantom Writes
**Definition**: Everything in memory must be explainable by WAL entries.

### M1.6: No Missing Writes
**Definition**: Everything in WAL must appear in memory after replay.

### M1.7: Replay Idempotence
**Definition**: Replaying WAL multiple times produces same state.
```
replay(replay(WAL)) == replay(WAL)
```

### M1.8: Delete Durability
**Definition**: Deleted keys stay deleted after recovery.

---

## M2 Invariants (Transactions + OCC + Snapshots)

M2 is: "OCC + snapshot isolation + conflict detection"

It is **NOT**:
- Full serializability
- Distributed semantics
- Linearizability
- Deterministic scheduling

### M2.1: Atomicity
**Definition**: A transaction's writes either all succeed (commit) or all fail (abort). No partial application.

### M2.2: Snapshot Isolation - No Dirty Reads
**Definition**: A transaction never observes another transaction's uncommitted writes.

### M2.3: Snapshot Isolation - Repeatable Reads
**Definition**: Same key returns same value within a transaction (snapshot immutability).

### M2.4: Snapshot Isolation - Read-Your-Writes
**Definition**: A transaction always sees its own uncommitted modifications.

### M2.5: Snapshot Consistency
**Definition**: All reads within a transaction see data from a single consistent point in time.

### M2.6: No Partial Visibility
**Definition**: Other transactions never see partial writes from an in-progress transaction.

### M2.7: Conflict Detection - Read-Write
**Definition**: If T1 reads key K at version V, and K's version changes before T1 commits, T1 aborts.

### M2.8: Conflict Detection - CAS
**Definition**: CAS succeeds only if current version matches expected version. Exactly one concurrent CAS wins.

### M2.9: First-Committer-Wins
**Definition**: When transactions conflict, the first to commit wins. Later transactions must abort.

### M2.10: Blind Write Behavior (from §3.2)
**Definition**: Blind writes (write without prior read) do NOT conflict. Both succeed, last writer wins.

### M2.11: CAS Read-Set Independence (from §3.4)
**Definition**: CAS does NOT add to read-set. Only validates expected_version.

### M2.12: Version 0 Semantics (from §6.4)
**Definition**: Version 0 means "key has never existed". CAS(v=0) is "insert if not exists".

### M2.13: Tombstone Semantics (from §6.5)
**Definition**: Deleted keys have version > 0 (tombstone). Different from never-existed (v=0).

### M2.14: Incomplete Transaction Discard (from §5.5)
**Definition**: Transactions without CommitTxn marker are discarded on recovery.

### M2.15: Lost Update Prevention
**Definition**: SI MUST prevent lost updates. If T1 and T2 both read-modify-write same key, one must abort.

### M2.16: Write Skew Allowance
**Definition**: SI ALLOWS write skew. This is intended behavior, not a bug.

---

## Test Classification

### Tier 1: Core Invariants (sacred, fast, must pass)
These enforce fundamental correctness. Run on every commit.

| Test | Invariant |
|------|-----------|
| `test_full_state_preserved_after_recovery` | M1.3 |
| `test_replay_twice_produces_identical_state` | M1.7 |
| `test_deletes_are_durable` | M1.8 |
| `test_committed_transaction_all_keys_present` | M2.1 |
| `test_aborted_transaction_no_keys_present` | M2.1 |
| `test_uncommitted_writes_invisible` | M2.2 |
| `test_repeated_reads_return_same_value` | M2.3 |
| `test_sees_own_puts` | M2.4 |
| `test_cas_exactly_one_winner` | M2.8 |
| `test_blind_write_both_succeed_last_wins` | M2.10 |
| `test_version_0_means_never_existed` | M2.12 |
| `test_lost_update_prevented_counter_increment` | M2.15 |

### Tier 2: Behavioral Scenarios (medium, workflow tests)
These test complete workflows. Run on every commit.

| Test | Purpose |
|------|---------|
| `test_transaction_workflow_*` | End-to-end transaction flows |
| `test_recovery_*` | Crash/recovery workflows |
| `test_conflict_detection_*` | Conflict scenarios |

### Tier 3: Stress/Chaos (opt-in, slow)
These find rare bugs. NOT run on every commit.

| Test | Purpose |
|------|---------|
| `test_concurrent_stress_*` | Race conditions |
| `test_throughput_*` | Performance cliffs |
| `test_many_*` | High volume |

---

## Anti-Patterns to Avoid

1. **Testing M2+ concepts in M1 tests**
   - M1 should not mention "isolation", "atomicity", "snapshot"
   - M1 is single-threaded, deterministic, boring

2. **Testing the same invariant multiple ways**
   - One test per invariant is ideal
   - Multiple tests for same invariant = redundancy

3. **Stress tests masquerading as correctness tests**
   - Stress tests find bugs but don't prove correctness
   - They should be opt-in, not blocking

4. **Testing implementation rather than contract**
   - Don't test WAL byte layout
   - Don't test internal data structures
   - Test observable behavior only

---

## Spec References

Each M2 invariant maps to M2_TRANSACTION_SEMANTICS.md:

| Invariant | Spec Section |
|-----------|--------------|
| M2.1 | Core Invariants: "All-or-nothing commit" |
| M2.2 | §2.2: What a Transaction NEVER Sees |
| M2.3 | §2.1: Snapshot consistency |
| M2.4 | §2.1: Read-your-writes |
| M2.5 | Core Invariants: "Snapshot consistency" |
| M2.6 | Core Invariants: "No partial commits" |
| M2.7 | §3.1: Read-Write Conflict |
| M2.8 | §3.1: CAS Conflict |
| M2.9 | §3.3: First-Committer-Wins |
| M2.10 | §3.2: Blind Write |
| M2.11 | §3.4: CAS Interaction |
| M2.12 | §6.4: Version 0 Semantics |
| M2.13 | §6.5: Tombstone Semantics |
| M2.14 | §5.5: Incomplete Transaction Handling |
| M2.15 | §1: No lost updates |
| M2.16 | §1: Write skew allowed |
