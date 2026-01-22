# Epic 44: Cross-Primitive Atomicity - Implementation Prompts

**Epic Goal**: Ensure transactions spanning primitives are atomic

**GitHub Issue**: [#342](https://github.com/anibjoshi/in-mem/issues/342)
**Status**: Ready to begin (after Epic 42 complete)
**Dependencies**: Epic 42 (WAL Enhancement)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M7_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M7_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M7/EPIC_44_CROSS_PRIMITIVE.md`
3. **Prompt Header**: `docs/prompts/M7/M7_PROMPT_HEADER.md` for the 5 architectural rules

---

## Epic 44 Overview

### Scope
- Transaction grouping in WAL (shared tx_id)
- Atomic commit (all entries or none)
- Recovery respects transaction boundaries
- Cross-primitive transaction tests

### The Core Guarantee

> After crash recovery, the database must correspond to a **prefix of the committed transaction history**. No partial transactions may be visible. If a transaction spans KV + JSON + Event + State, after crash recovery you must see either all effects or none.

### Key Rules

1. **All entries in a transaction share the same tx_id**
2. **Commit marker required for transaction to be visible**
3. **Recovery only applies entries with commit markers**
4. **Orphaned transactions (no commit) discarded during recovery**

### Success Criteria
- [ ] All entries in a transaction share tx_id
- [ ] Commit marker required for visibility
- [ ] Recovery only applies committed entries
- [ ] Transactions spanning KV + JSON + Event + State recover atomically
- [ ] Orphaned transactions not visible after recovery

### Component Breakdown
- **Story #317 (GitHub #372)**: Transaction Grouping in WAL - CRITICAL
- **Story #318 (GitHub #373)**: Atomic Commit (All or Nothing) - CRITICAL
- **Story #319 (GitHub #374)**: Recovery Respects Transaction Boundaries - CRITICAL
- **Story #320 (GitHub #375)**: Cross-Primitive Transaction Tests - HIGH

---

## Dependency Graph

```
Story #372 (Grouping) ──> Story #373 (Atomic Commit) ──> Story #374 (Recovery)
                                                                │
                                                                v
                                                         Story #375 (Tests)
```

---

## Story #372: Transaction Grouping in WAL

**GitHub Issue**: [#372](https://github.com/anibjoshi/in-mem/issues/372)
**Estimated Time**: 3 hours
**Dependencies**: Epic 42 complete
**Blocks**: Story #373

### Start Story

```bash
gh issue view 372
./scripts/start-story.sh 44 372 tx-grouping
```

### Implementation

```rust
/// Transaction that can span multiple primitives
///
/// All operations share the same tx_id.
/// Only visible after commit marker is written.
pub struct Transaction {
    id: TxId,
    entries: Vec<TxEntry>,
}

/// Entry types for transactions
pub enum TxEntry {
    KvPut { key: Key, value: Value },
    KvDelete { key: Key },
    JsonSet { key: Key, doc: JsonDoc },
    JsonDelete { key: Key },
    JsonPatch { key: Key, patch: JsonPatch },
    EventAppend { event: Event },
    StateSet { key: Key, value: StateValue },
    StateTransition { key: Key, from: StateValue, to: StateValue },
    TraceRecord { span: Span },
}

impl Transaction {
    pub fn new() -> Self {
        Transaction {
            id: TxId::new_v4(),
            entries: Vec::new(),
        }
    }

    pub fn id(&self) -> TxId {
        self.id
    }

    pub fn entries(&self) -> &[TxEntry] {
        &self.entries
    }

    /// Add KV put
    pub fn kv_put(&mut self, key: Key, value: Value) -> &mut Self {
        self.entries.push(TxEntry::KvPut { key, value });
        self
    }

    /// Add KV delete
    pub fn kv_delete(&mut self, key: Key) -> &mut Self {
        self.entries.push(TxEntry::KvDelete { key });
        self
    }

    /// Add JSON set
    pub fn json_set(&mut self, key: Key, doc: JsonDoc) -> &mut Self {
        self.entries.push(TxEntry::JsonSet { key, doc });
        self
    }

    /// Add JSON delete
    pub fn json_delete(&mut self, key: Key) -> &mut Self {
        self.entries.push(TxEntry::JsonDelete { key });
        self
    }

    /// Add JSON patch
    pub fn json_patch(&mut self, key: Key, patch: JsonPatch) -> &mut Self {
        self.entries.push(TxEntry::JsonPatch { key, patch });
        self
    }

    /// Add event
    pub fn event_append(&mut self, event: Event) -> &mut Self {
        self.entries.push(TxEntry::EventAppend { event });
        self
    }

    /// Add state set
    pub fn state_set(&mut self, key: Key, value: StateValue) -> &mut Self {
        self.entries.push(TxEntry::StateSet { key, value });
        self
    }

    /// Add state transition
    pub fn state_transition(&mut self, key: Key, from: StateValue, to: StateValue) -> &mut Self {
        self.entries.push(TxEntry::StateTransition { key, from, to });
        self
    }

    /// Add trace span
    pub fn trace_record(&mut self, span: Span) -> &mut Self {
        self.entries.push(TxEntry::TraceRecord { span });
        self
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl TxEntry {
    /// Convert to WAL entry type
    pub fn wal_entry_type(&self) -> WalEntryType {
        match self {
            TxEntry::KvPut { .. } => WalEntryType::KvPut,
            TxEntry::KvDelete { .. } => WalEntryType::KvDelete,
            TxEntry::JsonSet { .. } => WalEntryType::JsonSet,
            TxEntry::JsonDelete { .. } => WalEntryType::JsonDelete,
            TxEntry::JsonPatch { .. } => WalEntryType::JsonPatch,
            TxEntry::EventAppend { .. } => WalEntryType::EventAppend,
            TxEntry::StateSet { .. } => WalEntryType::StateSet,
            TxEntry::StateTransition { .. } => WalEntryType::StateTransition,
            TxEntry::TraceRecord { .. } => WalEntryType::TraceRecord,
        }
    }

    /// Serialize entry payload
    pub fn serialize(&self) -> Vec<u8> {
        match self {
            TxEntry::KvPut { key, value } => {
                let mut buf = Vec::new();
                buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
                buf.extend_from_slice(key.as_bytes());
                buf.extend_from_slice(&value);
                buf
            }
            // Similar for other entry types...
            _ => vec![]
        }
    }
}
```

### Acceptance Criteria

- [ ] Transaction builder with all primitive operations
- [ ] All entries share same tx_id
- [ ] TxEntry converts to WalEntryType
- [ ] Serialize produces correct payload

### Complete Story

```bash
./scripts/complete-story.sh 372
```

---

## Story #373: Atomic Commit (All or Nothing)

**GitHub Issue**: [#373](https://github.com/anibjoshi/in-mem/issues/373)
**Estimated Time**: 3 hours
**Dependencies**: Story #372

### Start Story

```bash
gh issue view 373
./scripts/start-story.sh 44 373 atomic-commit
```

### Implementation

```rust
impl Database {
    /// Commit a transaction atomically
    ///
    /// 1. Write all entries to WAL with shared tx_id
    /// 2. Write commit marker
    /// 3. Apply to in-memory state
    ///
    /// If crash between steps 1 and 2, transaction is discarded on recovery.
    /// If crash after step 2, transaction is replayed on recovery.
    pub fn commit(&self, tx: Transaction) -> Result<(), CommitError> {
        if tx.is_empty() {
            return Ok(());
        }

        let tx_id = tx.id();

        // Phase 1: Write to WAL
        self.wal.write_transaction(&tx)?;

        // Phase 2: Apply to in-memory state
        // This is safe because WAL already has commit marker
        for entry in tx.entries() {
            self.apply_entry(entry)?;
        }

        Ok(())
    }

    /// Apply entry to in-memory state
    fn apply_entry(&self, entry: &TxEntry) -> Result<(), CommitError> {
        match entry {
            TxEntry::KvPut { key, value } => {
                self.kv.put_raw(key.clone(), value.clone())?;
            }
            TxEntry::KvDelete { key } => {
                self.kv.delete_raw(key.clone())?;
            }
            TxEntry::JsonSet { key, doc } => {
                self.json.set_raw(key.clone(), doc.clone())?;
            }
            TxEntry::JsonDelete { key } => {
                self.json.delete_raw(key.clone())?;
            }
            TxEntry::JsonPatch { key, patch } => {
                self.json.apply_patch_raw(key.clone(), patch.clone())?;
            }
            TxEntry::EventAppend { event } => {
                self.event.append_raw(event.clone())?;
            }
            TxEntry::StateSet { key, value } => {
                self.state.set_raw(key.clone(), value.clone())?;
            }
            TxEntry::StateTransition { key, from, to } => {
                self.state.transition_raw(key.clone(), from.clone(), to.clone())?;
            }
            TxEntry::TraceRecord { span } => {
                self.trace.record_raw(span.clone())?;
            }
        }
        Ok(())
    }
}

/// Commit errors
#[derive(Debug, thiserror::Error)]
pub enum CommitError {
    #[error("WAL error: {0}")]
    Wal(#[from] WalError),

    #[error("KV error: {0}")]
    Kv(String),

    #[error("JSON error: {0}")]
    Json(String),

    #[error("State error: {0}")]
    State(String),
}
```

### Acceptance Criteria

- [ ] All entries written to WAL first
- [ ] Commit marker written after all entries
- [ ] In-memory state updated after WAL
- [ ] Crash before commit = transaction discarded
- [ ] Crash after commit = transaction replayed

### Complete Story

```bash
./scripts/complete-story.sh 373
```

---

## Story #374: Recovery Respects Transaction Boundaries

**GitHub Issue**: [#374](https://github.com/anibjoshi/in-mem/issues/374)
**Estimated Time**: 3 hours
**Dependencies**: Story #373

### Start Story

```bash
gh issue view 374
./scripts/start-story.sh 44 374 tx-recovery
```

### Implementation

This is primarily verification that Epic 41 (Story #356: WAL Replay from Offset) correctly handles transaction boundaries. Add explicit tests:

```rust
#[cfg(test)]
mod cross_primitive_recovery_tests {
    use super::*;

    #[test]
    fn test_cross_primitive_atomic_recovery() {
        let temp_dir = TempDir::new().unwrap();

        // Create transaction spanning KV + JSON + Event + State
        {
            let db = create_test_db(temp_dir.path());

            let mut tx = Transaction::new();
            tx.kv_put("key1".into(), "value1".into())
              .json_set("doc1".into(), json!({"field": "value"}))
              .event_append(Event::new("task_started"))
              .state_set("state1".into(), "active".into());

            db.commit(tx).unwrap();
        }

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // All or nothing: should have all 4 effects
        assert!(recovered.kv.get("key1").is_some());
        assert!(recovered.json.get("doc1").is_some());
        assert!(!recovered.event.get_all().is_empty());
        assert!(recovered.state.get("state1").is_some());
    }

    #[test]
    fn test_uncommitted_tx_not_visible() {
        let temp_dir = TempDir::new().unwrap();

        // Write entries without commit marker (simulate crash mid-transaction)
        {
            let db = create_test_db(temp_dir.path());

            let tx_id = TxId::new_v4();

            // Write entries but no commit marker
            let entry1 = WalEntry {
                entry_type: WalEntryType::KvPut,
                version: 1,
                tx_id: Some(tx_id),
                payload: serialize_kv_put("key1", "value1"),
            };
            db.wal.write_entry(&entry1).unwrap();

            let entry2 = WalEntry {
                entry_type: WalEntryType::JsonSet,
                version: 1,
                tx_id: Some(tx_id),
                payload: serialize_json_set("doc1", json!({})),
            };
            db.wal.write_entry(&entry2).unwrap();

            // No commit marker - simulating crash
        }

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // Should have NO effects (transaction was not committed)
        assert!(recovered.kv.get("key1").is_none());
        assert!(recovered.json.get("doc1").is_none());
        assert_eq!(result.orphaned_transactions, 1);
    }

    #[test]
    fn test_partial_tx_not_visible() {
        let temp_dir = TempDir::new().unwrap();

        // Transaction 1: committed
        // Transaction 2: partially written (no commit marker)
        {
            let db = create_test_db(temp_dir.path());

            // Transaction 1 - complete
            let mut tx1 = Transaction::new();
            tx1.kv_put("tx1_key".into(), "tx1_value".into());
            db.commit(tx1).unwrap();

            // Transaction 2 - partial (no commit)
            let tx2_id = TxId::new_v4();
            let entry = WalEntry {
                entry_type: WalEntryType::KvPut,
                version: 1,
                tx_id: Some(tx2_id),
                payload: serialize_kv_put("tx2_key", "tx2_value"),
            };
            db.wal.write_entry(&entry).unwrap();
            // No commit marker
        }

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // TX1 should be visible, TX2 should not
        assert!(recovered.kv.get("tx1_key").is_some());
        assert!(recovered.kv.get("tx2_key").is_none());
        assert_eq!(result.transactions_recovered, 1);
        assert_eq!(result.orphaned_transactions, 1);
    }
}
```

### Acceptance Criteria

- [ ] Committed transactions fully recovered
- [ ] Uncommitted transactions discarded
- [ ] No partial transactions visible
- [ ] OrphanedTransactions count accurate

### Complete Story

```bash
./scripts/complete-story.sh 374
```

---

## Story #375: Cross-Primitive Transaction Tests

**GitHub Issue**: [#375](https://github.com/anibjoshi/in-mem/issues/375)
**Estimated Time**: 3 hours
**Dependencies**: Story #374

### Start Story

```bash
gh issue view 375
./scripts/start-story.sh 44 375 cross-primitive-tests
```

### Implementation

Create comprehensive test suite:

```rust
#[cfg(test)]
mod cross_primitive_integration_tests {
    use super::*;

    /// Test: All 6 primitives in one transaction
    #[test]
    fn test_all_primitives_atomic() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            let mut tx = Transaction::new();
            tx.kv_put("kv_key".into(), "kv_value".into())
              .json_set("json_key".into(), json!({"hello": "world"}))
              .event_append(Event::new("event1"))
              .state_set("state_key".into(), "running".into())
              .trace_record(Span::new("operation1"));

            db.commit(tx).unwrap();
        }

        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        assert!(recovered.kv.get("kv_key").is_some());
        assert!(recovered.json.get("json_key").is_some());
        assert!(!recovered.event.get_all().is_empty());
        assert!(recovered.state.get("state_key").is_some());
        assert_eq!(result.transactions_recovered, 1);
    }

    /// Test: Multiple transactions, interleaved
    #[test]
    fn test_interleaved_transactions() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            // TX1
            let mut tx1 = Transaction::new();
            tx1.kv_put("tx1_kv".into(), "value1".into())
               .json_set("tx1_json".into(), json!({}));
            db.commit(tx1).unwrap();

            // TX2
            let mut tx2 = Transaction::new();
            tx2.kv_put("tx2_kv".into(), "value2".into())
               .state_set("tx2_state".into(), "active".into());
            db.commit(tx2).unwrap();

            // TX3
            let mut tx3 = Transaction::new();
            tx3.json_set("tx3_json".into(), json!({"x": 1}))
               .event_append(Event::new("tx3_event"));
            db.commit(tx3).unwrap();
        }

        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // All 3 transactions should be recovered
        assert_eq!(result.transactions_recovered, 3);
        assert!(recovered.kv.get("tx1_kv").is_some());
        assert!(recovered.kv.get("tx2_kv").is_some());
        assert!(recovered.json.get("tx1_json").is_some());
        assert!(recovered.json.get("tx3_json").is_some());
        assert!(recovered.state.get("tx2_state").is_some());
    }

    /// Test: Crash mid-transaction leaves no trace
    #[test]
    fn test_crash_mid_transaction_recovery() {
        let temp_dir = TempDir::new().unwrap();

        // Simulate: TX1 committed, TX2 in progress when crash
        {
            let db = create_test_db(temp_dir.path());

            // TX1 - committed
            let mut tx1 = Transaction::new();
            tx1.kv_put("committed_key".into(), "value".into());
            db.commit(tx1).unwrap();

            // TX2 - in progress (write some entries, then "crash" before commit)
            let tx2_id = TxId::new_v4();

            // Write 3 entries for TX2
            for i in 0..3 {
                let entry = WalEntry {
                    entry_type: WalEntryType::KvPut,
                    version: 1,
                    tx_id: Some(tx2_id),
                    payload: serialize_kv_put(&format!("pending_{}", i), "value"),
                };
                db.wal.write_entry(&entry).unwrap();
            }
            // "Crash" - no commit marker
        }

        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // TX1 visible, TX2 not visible
        assert!(recovered.kv.get("committed_key").is_some());
        assert!(recovered.kv.get("pending_0").is_none());
        assert!(recovered.kv.get("pending_1").is_none());
        assert!(recovered.kv.get("pending_2").is_none());

        assert_eq!(result.transactions_recovered, 1);
        assert_eq!(result.orphaned_transactions, 1);
    }

    /// Test: Large transaction with many entries
    #[test]
    fn test_large_transaction() {
        let temp_dir = TempDir::new().unwrap();

        {
            let db = create_test_db(temp_dir.path());

            let mut tx = Transaction::new();
            for i in 0..1000 {
                tx.kv_put(format!("key_{}", i).into(), format!("value_{}", i).into());
            }
            db.commit(tx).unwrap();
        }

        let (recovered, result) = RecoveryEngine::recover(
            temp_dir.path(),
            RecoveryOptions::default(),
        ).unwrap();

        // All 1000 keys should be present
        for i in 0..1000 {
            assert!(recovered.kv.get(&format!("key_{}", i)).is_some());
        }
        assert_eq!(result.transactions_recovered, 1);
    }
}
```

### Acceptance Criteria

- [ ] All 6 primitives in one transaction test
- [ ] Interleaved transactions test
- [ ] Crash mid-transaction test
- [ ] Large transaction test
- [ ] All tests pass

### Complete Story

```bash
./scripts/complete-story.sh 375
```

---

## Epic 44 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test --workspace -- cross_primitive
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] Transaction builder with all primitives
- [ ] Atomic commit via WAL
- [ ] Recovery respects boundaries
- [ ] Comprehensive test suite

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-44-cross-primitive -m "Epic 44: Cross-Primitive Atomicity complete

Delivered:
- Transaction grouping with shared tx_id
- Atomic commit via WAL with commit markers
- Recovery respects transaction boundaries
- Comprehensive cross-primitive tests

Stories: #372, #373, #374, #375
"
git push origin develop
gh issue close 342 --comment "Epic 44: Cross-Primitive Atomicity - COMPLETE"
```

---

## Summary

Epic 44 ensures that transactions spanning multiple primitives are atomic. After crash recovery, you see either all effects of a transaction or none. This is the core guarantee that makes the database trustworthy for agents.
