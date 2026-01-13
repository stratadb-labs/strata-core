# Epic 12: Comprehensive Test Failures Investigation

## Branch: `fix/issues-148-153`

## Overview

After running the comprehensive M1/M2 integration tests (`tests/m1_m2_comprehensive/`), six issues were discovered. These fall into two categories:

### Category A: Transaction Snapshot Isolation Issues (#148, #152)
Transactions calling `txn.get()` return `None` for keys that definitely exist in storage.

### Category B: WAL Recovery Data Loss (#149, #150, #151, #153)
Keys written to the database are not fully recovered after close/reopen cycle. Only a fraction of keys survive.

---

## Issues Summary

| Issue | Test | Error | Category |
|-------|------|-------|----------|
| #148 | `test_acid_counter_increment` | `unwrap()` on `None` in `txn.get()` | A |
| #149 | `test_concurrent_writes_all_recovered` | Keys missing after recovery | B |
| #150 | `test_large_values_recovered` | Large value missing after recovery | B |
| #151 | `test_many_keys_recovered` | Partial keys recovered (1000 written) | B |
| #152 | `test_bank_transfer_sum_invariant` | `unwrap()` on `None` in `txn.get()` | A |
| #153 | `test_large_state_fully_preserved` | Only 259 of 1000 keys recovered | B |

---

## Category A: Transaction Snapshot Isolation Issues

### Symptoms
- `txn.get(&key)` returns `None` for keys that exist in storage
- Occurs in concurrent transaction scenarios
- Tests: #148, #152

### Investigation Steps

1. **Read the transaction snapshot mechanism**:
   - `crates/concurrency/src/transaction.rs` - `TransactionContext::get()`
   - `crates/storage/src/snapshot.rs` - How snapshots are created
   - `crates/storage/src/unified.rs` - `UnifiedStore::snapshot()`

2. **Understand the flow**:
   ```
   db.begin_transaction()
     -> coordinator.begin_transaction()
     -> storage.snapshot()
     -> TransactionContext with snapshot
   ```

3. **Check for race conditions**:
   - Is the snapshot taken at the correct point in time?
   - Is there a window where a commit is in-progress but snapshot doesn't see it?
   - Does `commit_lock` protect the storage update atomically?

4. **Verify snapshot isolation**:
   - Per spec Section 3.2: Transactions should see a consistent snapshot
   - The snapshot should include all committed data at the start of the transaction

### Hypothesis
The `Database::get()` optimization (Issue #141 fix) reads directly from storage, but `TransactionContext::get()` reads from the snapshot. If there's a timing issue where:
1. Thread A commits and updates storage
2. Thread B starts transaction, takes snapshot BEFORE storage update completes
3. Thread B's `txn.get()` returns `None` because snapshot doesn't have the key

Check if `commit_lock` is held during the entire commit sequence including storage update.

### Key Files to Examine
- `crates/engine/src/database.rs:526-570` - `commit_transaction()` implementation
- `crates/storage/src/unified.rs` - `apply_write()`, `snapshot()`
- `crates/concurrency/src/transaction.rs` - `TransactionContext::get()`

---

## Category B: WAL Recovery Data Loss

### Symptoms
- Only ~25% of written keys survive recovery (259 of 1000)
- Affects both sequential and concurrent writes
- Tests: #149, #150, #151, #153

### Investigation Steps

1. **Verify WAL is being written correctly**:
   - Add debug logging to count WAL entries before close
   - Check if `fsync()` is being called correctly
   - Verify `DurabilityMode::Strict` is actually syncing

2. **Verify WAL replay is correct**:
   - We fixed Issue #145 (duplicate txn_id) - is that fix working?
   - Check `crates/durability/src/recovery.rs` - `replay_wal()`
   - Count transactions in WAL vs transactions replayed

3. **Test the WAL directly**:
   ```rust
   // After close, before reopen:
   let wal = WAL::open(&wal_path, mode)?;
   let entries = wal.read_all()?;
   println!("WAL has {} entries", entries.len());
   // Count BeginTxn, CommitTxn entries
   ```

4. **Check for batched writes issue**:
   - `DurabilityMode::Batched` vs `DurabilityMode::Strict`
   - Is the WAL buffer being flushed?
   - Does `Database::close()` or `Drop` properly flush?

### Hypothesis
The tests may be using `DurabilityMode::Batched` (default) which doesn't fsync on every write. When the database is dropped:
1. WAL has entries in memory buffer
2. `Drop::drop()` calls `fsync()` but buffer may not be fully written
3. On recovery, only the entries that made it to disk are replayed

Check:
- What `DurabilityMode` are the tests using?
- Is `Database::open()` defaulting to `Batched`?
- Does `close()` flush the WAL buffer completely?

### Key Files to Examine
- `crates/durability/src/wal.rs` - `WAL::append()`, `fsync()`, buffer management
- `crates/engine/src/database.rs:732-735` - `close()` implementation
- `crates/engine/src/database.rs:738-755` - `Drop` implementation
- `crates/durability/src/recovery.rs:369-478` - `replay_wal()`

---

## Reproduction Commands

```bash
# Run all failing tests
cargo test --test m1_m2_comprehensive test_acid_counter_increment
cargo test --test m1_m2_comprehensive test_concurrent_writes_all_recovered
cargo test --test m1_m2_comprehensive test_large_values_recovered
cargo test --test m1_m2_comprehensive test_many_keys_recovered
cargo test --test m1_m2_comprehensive test_bank_transfer_sum_invariant
cargo test --test m1_m2_comprehensive test_large_state_fully_preserved

# Run with output
cargo test --test m1_m2_comprehensive test_large_state_fully_preserved -- --nocapture
```

---

## Fix Strategy

### For Category A (Snapshot Issues):
1. Ensure `commit_lock` is held during the ENTIRE commit: validate + WAL + storage update
2. Verify snapshot is taken after acquiring any necessary read locks
3. Consider if storage updates need memory barriers for visibility

### For Category B (Recovery Issues):
1. Ensure tests use `DurabilityMode::Strict` for reliable recovery testing
2. Verify `close()` calls `fsync()` AND waits for completion
3. Check WAL buffer flush logic in both normal close and Drop
4. Add explicit `db.close()` calls in tests before reopening

---

## Definition of Done

- [ ] All 6 tests pass consistently (run 10x without failure)
- [ ] Root cause documented for each category
- [ ] No regression in existing tests
- [ ] PR created and merged to develop
