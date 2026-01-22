# Epic 44: Cross-Primitive Atomicity

**Goal**: Ensure transactions spanning primitives are atomic

**Dependencies**: Epic 42 (WAL Enhancement)

---

## Scope

- Transaction grouping in WAL
- Atomic commit (all or nothing)
- Recovery respects transaction boundaries
- Cross-primitive transaction tests

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #317 | Transaction Grouping in WAL | CRITICAL |
| #318 | Atomic Commit (All or Nothing) | CRITICAL |
| #319 | Recovery Respects Transaction Boundaries | CRITICAL |
| #320 | Cross-Primitive Transaction Tests | HIGH |

---

## Story #317: Transaction Grouping in WAL

**File**: `crates/concurrency/src/transaction.rs`

**Deliverable**: Transaction entries share TxId in WAL

### Implementation

```rust
use crate::wal::{WalEntry, WalEntryType, TxId, WalWriter};

impl TransactionContext {
    /// Convert transaction to WAL entries
    ///
    /// All entries share the same TxId for atomic grouping.
    pub fn to_wal_entries(&self) -> (TxId, Vec<WalEntry>) {
        let tx_id = TxId::new();
        let mut entries = Vec::new();

        // KV entries
        for write in &self.kv_writes {
            let (entry_type, payload) = match write {
                KvWrite::Put { key, value } => {
                    let payload = bincode::serialize(&(key, value)).unwrap();
                    (WalEntryType::KvPut, payload)
                }
                KvWrite::Delete { key } => {
                    let payload = bincode::serialize(key).unwrap();
                    (WalEntryType::KvDelete, payload)
                }
            };
            entries.push(WalEntry {
                entry_type,
                version: 1,
                tx_id,
                payload,
            });
        }

        // JSON entries
        for write in &self.json_writes {
            let (entry_type, payload) = match write {
                JsonWrite::Create { key, doc } => {
                    let payload = bincode::serialize(&(key, doc)).unwrap();
                    (WalEntryType::JsonCreate, payload)
                }
                JsonWrite::Set { key, doc } => {
                    let payload = bincode::serialize(&(key, doc)).unwrap();
                    (WalEntryType::JsonSet, payload)
                }
                JsonWrite::Delete { key } => {
                    let payload = bincode::serialize(key).unwrap();
                    (WalEntryType::JsonDelete, payload)
                }
                JsonWrite::Patch { key, patches } => {
                    let payload = bincode::serialize(&(key, patches)).unwrap();
                    (WalEntryType::JsonPatch, payload)
                }
            };
            entries.push(WalEntry {
                entry_type,
                version: 1,
                tx_id,
                payload,
            });
        }

        // Event entries
        for event in &self.events {
            let payload = bincode::serialize(event).unwrap();
            entries.push(WalEntry {
                entry_type: WalEntryType::EventAppend,
                version: 1,
                tx_id,
                payload,
            });
        }

        // State entries
        for write in &self.state_writes {
            let (entry_type, payload) = match write {
                StateWrite::Init { key, value } => {
                    let payload = bincode::serialize(&(key, value)).unwrap();
                    (WalEntryType::StateInit, payload)
                }
                StateWrite::Set { key, value } => {
                    let payload = bincode::serialize(&(key, value)).unwrap();
                    (WalEntryType::StateSet, payload)
                }
                StateWrite::Transition { key, from, to } => {
                    let payload = bincode::serialize(&(key, from, to)).unwrap();
                    (WalEntryType::StateTransition, payload)
                }
            };
            entries.push(WalEntry {
                entry_type,
                version: 1,
                tx_id,
                payload,
            });
        }

        // Trace entries
        for span in &self.traces {
            let payload = bincode::serialize(span).unwrap();
            entries.push(WalEntry {
                entry_type: WalEntryType::TraceRecord,
                version: 1,
                tx_id,
                payload,
            });
        }

        (tx_id, entries)
    }
}
```

### Acceptance Criteria

- [ ] All entries in transaction share TxId
- [ ] KV, JSON, Event, State, Trace entries created
- [ ] Entry type matches operation
- [ ] Payload serialized correctly

---

## Story #318: Atomic Commit (All or Nothing)

**File**: `crates/engine/src/database.rs`

**Deliverable**: Commit writes all entries + commit marker

### Implementation

```rust
impl Database {
    /// Commit transaction atomically
    ///
    /// Writes all WAL entries followed by commit marker.
    /// Either all entries are durable, or none.
    pub fn commit_transaction(&self, tx: TransactionContext) -> Result<(), CommitError> {
        // Validate transaction
        self.validate_transaction(&tx)?;

        // Convert to WAL entries
        let (tx_id, entries) = tx.to_wal_entries();

        if entries.is_empty() {
            // Nothing to commit
            return Ok(());
        }

        // Write all entries
        for entry in &entries {
            self.wal.write_entry(entry)?;
        }

        // Write commit marker
        let commit_marker = WalEntry {
            entry_type: WalEntryType::TransactionCommit,
            version: 1,
            tx_id,
            payload: vec![],
        };
        self.wal.write_entry(&commit_marker)?;

        // Sync based on durability mode
        match self.durability_mode {
            DurabilityMode::Strict => {
                self.wal.sync()?;
            }
            DurabilityMode::Buffered => {
                // Will sync later in background
            }
            DurabilityMode::InMemory => {
                // No sync needed
            }
        }

        // Apply to in-memory state
        self.apply_transaction(&tx)?;

        // Record events for replay
        self.record_transaction_events(&tx)?;

        Ok(())
    }

    /// Apply transaction to in-memory state
    fn apply_transaction(&self, tx: &TransactionContext) -> Result<(), CommitError> {
        // KV
        for write in &tx.kv_writes {
            match write {
                KvWrite::Put { key, value } => self.kv.put_raw(key.clone(), value.clone())?,
                KvWrite::Delete { key } => self.kv.delete_raw(key.clone())?,
            }
        }

        // JSON
        for write in &tx.json_writes {
            match write {
                JsonWrite::Create { key, doc } => self.json.create_raw(key.clone(), doc.clone())?,
                JsonWrite::Set { key, doc } => self.json.set_raw(key.clone(), doc.clone())?,
                JsonWrite::Delete { key } => self.json.delete_raw(key.clone())?,
                JsonWrite::Patch { key, patches } => self.json.patch_raw(key.clone(), patches.clone())?,
            }
        }

        // Event
        for event in &tx.events {
            self.event_log.append_raw(event.clone())?;
        }

        // State
        for write in &tx.state_writes {
            match write {
                StateWrite::Init { key, value } => self.state.init_raw(key.clone(), value.clone())?,
                StateWrite::Set { key, value } => self.state.set_raw(key.clone(), value.clone())?,
                StateWrite::Transition { key, from, to } => {
                    self.state.transition_raw(key.clone(), from.clone(), to.clone())?
                }
            }
        }

        // Trace
        for span in &tx.traces {
            self.trace.record_raw(span.clone())?;
        }

        Ok(())
    }

    /// Record events for run replay
    fn record_transaction_events(&self, tx: &TransactionContext) -> Result<(), CommitError> {
        let run_id = tx.run_id;

        // Record each operation as an event for replay
        for write in &tx.kv_writes {
            let event = match write {
                KvWrite::Put { key, value } => RunEvent::KvPut {
                    key: key.clone(),
                    value: value.clone(),
                },
                KvWrite::Delete { key } => RunEvent::KvDelete { key: key.clone() },
            };
            let offset = self.event_log.append_run_event(run_id, event)?;
            self.run_index.record_event(run_id, offset);
        }

        // Similar for JSON, Event, State, Trace...

        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] All entries written before commit marker
- [ ] Commit marker written after entries
- [ ] Sync based on durability mode
- [ ] Apply to in-memory state after WAL
- [ ] Record events for replay

---

## Story #319: Recovery Respects Transaction Boundaries

**File**: `crates/durability/src/recovery.rs`

**Deliverable**: Recovery only applies committed transactions

### Implementation

```rust
impl RecoveryEngine {
    /// Replay WAL respecting transaction boundaries
    ///
    /// CRITICAL: Only applies entries with commit markers.
    /// Entries without commit markers are orphaned and skipped.
    fn replay_wal_atomic(
        db: &mut Database,
        wal_path: &Path,
        from_offset: u64,
        options: &RecoveryOptions,
    ) -> Result<WalReplayResult, RecoveryError> {
        let mut result = WalReplayResult::default();
        let mut reader = WalReader::open(wal_path)?;
        reader.seek_to(from_offset)?;

        // Buffer entries by transaction
        let mut tx_buffers: HashMap<TxId, Vec<WalEntry>> = HashMap::new();

        while let Some(entry_result) = reader.next_entry() {
            let entry = match entry_result {
                Ok(e) => e,
                Err(WalError::ChecksumMismatch { .. }) => {
                    result.corrupt_entries += 1;
                    if result.corrupt_entries > options.max_corrupt_entries as u64 {
                        return Err(RecoveryError::TooManyCorruptEntries(result.corrupt_entries));
                    }
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            result.entries_replayed += 1;

            match entry.entry_type {
                WalEntryType::TransactionCommit => {
                    // Transaction is committed - apply all buffered entries
                    if let Some(entries) = tx_buffers.remove(&entry.tx_id) {
                        for e in entries {
                            db.apply_wal_entry(&e)?;
                        }
                        result.transactions_recovered += 1;

                        if options.verbose {
                            tracing::debug!(
                                "Recovered transaction {:?} ({} entries)",
                                entry.tx_id,
                                entries.len()
                            );
                        }
                    }
                }
                WalEntryType::TransactionAbort => {
                    // Transaction was aborted - discard buffered entries
                    if tx_buffers.remove(&entry.tx_id).is_some() {
                        result.orphaned_transactions += 1;
                        tracing::debug!("Discarded aborted transaction {:?}", entry.tx_id);
                    }
                }
                _ => {
                    // Buffer entry for its transaction
                    if !entry.tx_id.is_nil() {
                        tx_buffers
                            .entry(entry.tx_id)
                            .or_insert_with(Vec::new)
                            .push(entry);
                    } else {
                        // Non-transactional entry (e.g., run lifecycle)
                        db.apply_wal_entry(&entry)?;
                    }
                }
            }
        }

        // Remaining buffered entries are from incomplete transactions
        for (tx_id, entries) in tx_buffers {
            tracing::warn!(
                "Orphaned transaction {:?}: {} entries discarded",
                tx_id,
                entries.len()
            );
            result.orphaned_transactions += 1;
        }

        Ok(result)
    }
}
```

### Acceptance Criteria

- [ ] Buffers entries by TxId
- [ ] Only applies on commit marker
- [ ] Discards on abort marker
- [ ] Orphaned transactions logged and counted
- [ ] Non-transactional entries applied immediately

---

## Story #320: Cross-Primitive Transaction Tests

**File**: `tests/cross_primitive_atomicity.rs` (NEW)

**Deliverable**: Comprehensive tests for atomic cross-primitive transactions

### Implementation

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_primitive_commit() {
        let db = test_db();
        let run_id = RunId::new();

        db.begin_run(run_id).unwrap();

        // Transaction spanning KV, JSON, Event, State
        db.transaction(run_id, |tx| {
            tx.kv_put("key1", "value1")?;
            tx.json_set("doc1", json!({"field": "value"}))?;
            tx.event_append(Event::new("task_started"))?;
            tx.state_set("state1", "active")?;
            Ok(())
        }).unwrap();

        // All should be visible
        assert!(db.kv.get(run_id, "key1").unwrap().is_some());
        assert!(db.json.get(run_id, "doc1").unwrap().is_some());
        // Events and state also visible
    }

    #[test]
    fn test_cross_primitive_rollback() {
        let db = test_db();
        let run_id = RunId::new();

        db.begin_run(run_id).unwrap();

        // Transaction that fails
        let result = db.transaction(run_id, |tx| {
            tx.kv_put("key1", "value1")?;
            tx.json_set("doc1", json!({"field": "value"}))?;
            // Force conflict or error
            Err(TransactionError::Conflict)
        });

        assert!(result.is_err());

        // Nothing should be visible
        assert!(db.kv.get(run_id, "key1").unwrap().is_none());
        assert!(db.json.get(run_id, "doc1").unwrap().is_none());
    }

    #[test]
    fn test_cross_primitive_recovery() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Create DB and commit cross-primitive transaction
        {
            let db = create_db(data_dir);
            let run_id = RunId::new();

            db.begin_run(run_id).unwrap();

            db.transaction(run_id, |tx| {
                tx.kv_put("key1", "value1")?;
                tx.json_set("doc1", json!({"field": "value"}))?;
                tx.state_set("state1", "active")?;
                Ok(())
            }).unwrap();

            db.end_run(run_id).unwrap();

            // Snapshot
            db.snapshot().unwrap();
        }

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            data_dir,
            RecoveryOptions::default(),
        ).unwrap();

        // All primitives should have their data
        let run_id = /* get from somewhere */;
        assert!(recovered.kv.get(run_id, "key1").unwrap().is_some());
        assert!(recovered.json.get(run_id, "doc1").unwrap().is_some());
    }

    #[test]
    fn test_crash_mid_transaction() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Simulate crash mid-transaction
        {
            let db = create_db(data_dir);
            let run_id = RunId::new();

            db.begin_run(run_id).unwrap();

            // Write entries but don't commit
            let (tx_id, entries) = create_test_transaction(run_id);
            for entry in entries {
                db.wal.write_entry(&entry).unwrap();
            }
            // NO commit marker - simulating crash

            // Don't call db.snapshot() - we're simulating crash
        }

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            data_dir,
            RecoveryOptions::default(),
        ).unwrap();

        // Orphaned transaction should not be visible
        assert_eq!(result.orphaned_transactions, 1);
        // Data should not exist
    }

    #[test]
    fn test_partial_transaction_not_visible() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Create transaction with multiple primitives, crash before commit
        {
            let db = create_db(data_dir);
            let run_id = RunId::new();
            let tx_id = TxId::new();

            // Write KV entry
            db.wal.write_tx_entry(tx_id, WalEntryType::KvPut, b"key1=value1".to_vec()).unwrap();

            // Write JSON entry
            db.wal.write_tx_entry(tx_id, WalEntryType::JsonSet, b"doc1={...}".to_vec()).unwrap();

            // NO commit - crash
        }

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            data_dir,
            RecoveryOptions::default(),
        ).unwrap();

        // Neither KV nor JSON should be visible
        assert_eq!(result.orphaned_transactions, 1);
        // No partial state
    }

    #[test]
    fn test_recovery_deterministic() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Create and populate
        {
            let db = create_db(data_dir);
            for i in 0..100 {
                let run_id = RunId::new();
                db.begin_run(run_id).unwrap();
                db.transaction(run_id, |tx| {
                    tx.kv_put(format!("key{}", i), format!("value{}", i))?;
                    tx.json_set(format!("doc{}", i), json!({"i": i}))?;
                    Ok(())
                }).unwrap();
                db.end_run(run_id).unwrap();
            }
            db.snapshot().unwrap();
        }

        // Recover twice
        let (db1, _) = RecoveryEngine::recover(data_dir, RecoveryOptions::default()).unwrap();
        let (db2, _) = RecoveryEngine::recover(data_dir, RecoveryOptions::default()).unwrap();

        // Must be identical
        assert_eq!(db1.kv.list_all().unwrap(), db2.kv.list_all().unwrap());
        assert_eq!(db1.json.list_all().unwrap(), db2.json.list_all().unwrap());
    }
}
```

### Acceptance Criteria

- [ ] Cross-primitive commit works
- [ ] Cross-primitive rollback works
- [ ] Recovery recovers all primitives atomically
- [ ] Crash mid-transaction: nothing visible
- [ ] No partial transactions after recovery
- [ ] Recovery is deterministic

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/concurrency/src/transaction.rs` | MODIFY - to_wal_entries() |
| `crates/engine/src/database.rs` | MODIFY - commit_transaction() |
| `crates/durability/src/recovery.rs` | MODIFY - replay_wal_atomic() |
| `tests/cross_primitive_atomicity.rs` | CREATE - Integration tests |
