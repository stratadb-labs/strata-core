# Epic 14: KVStore Primitive - Implementation Prompts

**Epic Goal**: General-purpose key-value storage with run isolation.

**GitHub Issue**: [#160](https://github.com/anibjoshi/in-mem/issues/160)
**Status**: Ready to begin (after Epic 13)
**Dependencies**: Epic 13 (Primitives Foundation) complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M3_ARCHITECTURE.md` is the GOSPEL for ALL M3 implementation.**

Before starting ANY story in this epic, read:
- Section 4: KVStore Primitive
- Section 10: Transaction Integration
- Section 12: Invariant Enforcement

See `docs/prompts/M3_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 14 Overview

### Scope
- KVStore struct as stateless facade
- Single-operation API (implicit transactions): get, put, put_with_ttl, delete, list
- Multi-operation API (explicit transactions): KVTransaction
- List with prefix filtering
- Transaction extension trait: KVStoreExt

### Success Criteria
- [ ] KVStore struct implemented with `Arc<Database>` reference
- [ ] `get()` returns `Option<Value>` for key within run namespace
- [ ] `put()` stores value with TypeTag::KV prefix
- [ ] `put_with_ttl()` stores value with expiration metadata
- [ ] `delete()` removes key
- [ ] `list()` and `list_with_values()` support prefix filtering
- [ ] KVTransaction for multi-operation atomicity
- [ ] KVStoreExt trait for cross-primitive transactions
- [ ] Run isolation verified (different runs don't see each other's data)
- [ ] All unit tests pass (>95% coverage)

### Component Breakdown
- **Story #169**: KVStore Core Structure - BLOCKS others in this epic
- **Story #170**: KVStore Single-Operation API
- **Story #171**: KVStore Multi-Operation API
- **Story #172**: KVStore List Operations
- **Story #173**: KVStoreExt Transaction Extension

---

## Dependency Graph

```
Phase 1 (Sequential):
  Story #169 (KVStore Core Structure)
    └─> BLOCKS #170, #171, #172

Phase 2 (Parallel - 3 Claudes after #169):
  Story #170 (Single-Operation API)
  Story #171 (Multi-Operation API)
  Story #172 (List Operations)
    └─> All depend on #169
    └─> Independent of each other

Phase 3 (Sequential - after #170, #171, #172):
  Story #173 (KVStoreExt Transaction Extension)
    └─> Depends on all previous stories
```

---

## Parallelization Strategy

### Optimal Execution (3 Claudes)

| Phase | Duration | Claude 1 | Claude 2 | Claude 3 |
|-------|----------|----------|----------|----------|
| 1 | 3 hours | #169 Core | - | - |
| 2 | 4 hours | #170 Single-Op | #171 Multi-Op | #172 List |
| 3 | 3 hours | #173 Extension | - | - |

**Total Wall Time**: ~10 hours (vs. ~16 hours sequential)

---

## Story #169: KVStore Core Structure

**GitHub Issue**: [#169](https://github.com/anibjoshi/in-mem/issues/169)
**Estimated Time**: 3 hours
**Dependencies**: Epic 13 complete
**Blocks**: Stories #170, #171, #172

### Start Story

```bash
/opt/homebrew/bin/gh issue view 169
./scripts/start-story.sh 14 169 kvstore-core
```

### Implementation Steps

#### Step 1: Create KVStore struct

Create `crates/primitives/src/kv.rs`:

```rust
//! KVStore: General-purpose key-value storage primitive
//!
//! ## Design
//!
//! KVStore is a stateless facade over the Database engine. It holds no
//! in-memory state beyond an `Arc<Database>` reference.
//!
//! ## Run Isolation
//!
//! All operations are scoped to a `run_id`. Keys are prefixed with the
//! run's namespace, ensuring complete isolation between runs.
//!
//! ## Thread Safety
//!
//! KVStore is `Send + Sync` and can be safely shared across threads.
//! Multiple KVStore instances on the same Database are safe.

use std::sync::Arc;
use in_mem_engine::Database;
use in_mem_core::{Key, Namespace, RunId, TypeTag, Value, Result};

/// General-purpose key-value store primitive
///
/// Stateless facade over Database - all state lives in storage.
/// Multiple KVStore instances on same Database are safe.
#[derive(Clone)]
pub struct KVStore {
    db: Arc<Database>,
}

impl KVStore {
    /// Create new KVStore instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(run_id)
    }

    /// Build key for KV operation
    fn key_for(&self, run_id: &RunId, user_key: &str) -> Key {
        Key::new_kv(self.namespace_for_run(run_id), user_key)
    }
}
```

#### Step 2: Write core tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<Database>, KVStore) {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path()).unwrap());
        let kv = KVStore::new(db.clone());
        (temp_dir, db, kv)
    }

    #[test]
    fn test_kvstore_creation() {
        let (_temp, _db, kv) = setup();
        assert!(Arc::strong_count(kv.database()) >= 1);
    }

    #[test]
    fn test_kvstore_is_clone() {
        let (_temp, _db, kv1) = setup();
        let kv2 = kv1.clone();
        // Both point to same database
        assert!(Arc::ptr_eq(kv1.database(), kv2.database()));
    }

    #[test]
    fn test_kvstore_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<KVStore>();
    }

    #[test]
    fn test_key_construction() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();
        let key = kv.key_for(&run_id, "test-key");
        assert_eq!(key.type_tag, TypeTag::KV);
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-primitives -- kv
~/.cargo/bin/cargo clippy -p in-mem-primitives -- -D warnings
```

### Complete Story

```bash
./scripts/complete-story.sh 169
```

---

## Story #170: KVStore Single-Operation API

**GitHub Issue**: [#170](https://github.com/anibjoshi/in-mem/issues/170)
**Estimated Time**: 4 hours
**Dependencies**: Story #169

### Start Story

```bash
/opt/homebrew/bin/gh issue view 170
./scripts/start-story.sh 14 170 kvstore-single-op
```

### Implementation Steps

Add to `crates/primitives/src/kv.rs`:

```rust
use std::time::Duration;

impl KVStore {
    /// Get a value by key
    pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>> {
        self.db.transaction(run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.get(&storage_key)
        })
    }

    /// Put a value
    pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.put(storage_key, value)
        })
    }

    /// Put a value with TTL
    ///
    /// Note: TTL metadata is stored but cleanup is deferred to M4 background tasks.
    /// Reads will return expired values until cleanup runs.
    pub fn put_with_ttl(
        &self,
        run_id: &RunId,
        key: &str,
        value: Value,
        ttl: Duration,
    ) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            let expires_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
                + ttl.as_millis() as i64;

            // Store value with expiration metadata
            let value_with_ttl = Value::Map(std::collections::HashMap::from([
                ("value".to_string(), value),
                ("expires_at".to_string(), Value::I64(expires_at)),
            ]));

            txn.put(storage_key, value_with_ttl)
        })
    }

    /// Delete a key
    pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool> {
        self.db.transaction(run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.delete(&storage_key)
        })
    }

    /// Check if a key exists
    pub fn exists(&self, run_id: &RunId, key: &str) -> Result<bool> {
        Ok(self.get(run_id, key)?.is_some())
    }
}
```

### Tests

```rust
#[cfg(test)]
mod single_op_tests {
    use super::*;

    #[test]
    fn test_put_and_get() {
        let (_temp, db, kv) = setup();
        let run_id = RunId::new();
        db.begin_run(&run_id).unwrap();

        kv.put(&run_id, "key1", Value::String("value1".into())).unwrap();
        let result = kv.get(&run_id, "key1").unwrap();
        assert_eq!(result, Some(Value::String("value1".into())));
    }

    #[test]
    fn test_get_nonexistent() {
        let (_temp, db, kv) = setup();
        let run_id = RunId::new();
        db.begin_run(&run_id).unwrap();

        let result = kv.get(&run_id, "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete() {
        let (_temp, db, kv) = setup();
        let run_id = RunId::new();
        db.begin_run(&run_id).unwrap();

        kv.put(&run_id, "key1", Value::String("value1".into())).unwrap();
        assert!(kv.exists(&run_id, "key1").unwrap());

        kv.delete(&run_id, "key1").unwrap();
        assert!(!kv.exists(&run_id, "key1").unwrap());
    }

    #[test]
    fn test_run_isolation() {
        let (_temp, db, kv) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();
        db.begin_run(&run1).unwrap();
        db.begin_run(&run2).unwrap();

        kv.put(&run1, "shared-key", Value::String("run1-value".into())).unwrap();
        kv.put(&run2, "shared-key", Value::String("run2-value".into())).unwrap();

        // Each run sees its own value
        assert_eq!(
            kv.get(&run1, "shared-key").unwrap(),
            Some(Value::String("run1-value".into()))
        );
        assert_eq!(
            kv.get(&run2, "shared-key").unwrap(),
            Some(Value::String("run2-value".into()))
        );
    }

    #[test]
    fn test_put_with_ttl() {
        let (_temp, db, kv) = setup();
        let run_id = RunId::new();
        db.begin_run(&run_id).unwrap();

        kv.put_with_ttl(
            &run_id,
            "expiring-key",
            Value::String("temp".into()),
            Duration::from_secs(3600),
        ).unwrap();

        // Value is stored with metadata
        let result = kv.get(&run_id, "expiring-key").unwrap();
        assert!(result.is_some());
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 170
```

---

## Story #171: KVStore Multi-Operation API

**GitHub Issue**: [#171](https://github.com/anibjoshi/in-mem/issues/171)
**Estimated Time**: 3 hours
**Dependencies**: Story #169

### Start Story

```bash
/opt/homebrew/bin/gh issue view 171
./scripts/start-story.sh 14 171 kvstore-multi-op
```

### Implementation

Add to `crates/primitives/src/kv.rs`:

```rust
use in_mem_concurrency::TransactionContext;

/// Transaction handle for multi-key KV operations
pub struct KVTransaction<'a> {
    txn: &'a mut TransactionContext,
    run_id: RunId,
}

impl<'a> KVTransaction<'a> {
    /// Get a value within the transaction
    pub fn get(&mut self, key: &str) -> Result<Option<Value>> {
        let storage_key = Key::new_kv(Namespace::for_run(&self.run_id), key);
        self.txn.get(&storage_key)
    }

    /// Put a value within the transaction
    pub fn put(&mut self, key: &str, value: Value) -> Result<()> {
        let storage_key = Key::new_kv(Namespace::for_run(&self.run_id), key);
        self.txn.put(storage_key, value)
    }

    /// Delete a key within the transaction
    pub fn delete(&mut self, key: &str) -> Result<bool> {
        let storage_key = Key::new_kv(Namespace::for_run(&self.run_id), key);
        self.txn.delete(&storage_key)
    }
}

impl KVStore {
    /// Execute multiple KV operations atomically
    ///
    /// All operations within the closure are part of a single transaction.
    /// Either all succeed or all are rolled back.
    pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut KVTransaction<'_>) -> Result<T>,
    {
        self.db.transaction(run_id, |txn| {
            let mut kv_txn = KVTransaction {
                txn,
                run_id: run_id.clone(),
            };
            f(&mut kv_txn)
        })
    }
}
```

### Tests

```rust
#[test]
fn test_multi_key_atomic() {
    let (_temp, db, kv) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    kv.transaction(&run_id, |txn| {
        txn.put("key1", Value::String("val1".into()))?;
        txn.put("key2", Value::String("val2".into()))?;
        txn.put("key3", Value::String("val3".into()))?;
        Ok(())
    }).unwrap();

    assert_eq!(kv.get(&run_id, "key1").unwrap(), Some(Value::String("val1".into())));
    assert_eq!(kv.get(&run_id, "key2").unwrap(), Some(Value::String("val2".into())));
    assert_eq!(kv.get(&run_id, "key3").unwrap(), Some(Value::String("val3".into())));
}

#[test]
fn test_transaction_read_your_writes() {
    let (_temp, db, kv) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    kv.transaction(&run_id, |txn| {
        txn.put("key", Value::I64(1))?;
        let val = txn.get("key")?;
        assert_eq!(val, Some(Value::I64(1)));
        Ok(())
    }).unwrap();
}
```

### Complete Story

```bash
./scripts/complete-story.sh 171
```

---

## Story #172: KVStore List Operations

**GitHub Issue**: [#172](https://github.com/anibjoshi/in-mem/issues/172)
**Estimated Time**: 3 hours
**Dependencies**: Story #169

### Start Story

```bash
/opt/homebrew/bin/gh issue view 172
./scripts/start-story.sh 14 172 kvstore-list
```

### Implementation

```rust
impl KVStore {
    /// List keys with optional prefix filter
    pub fn list(&self, run_id: &RunId, prefix: Option<&str>) -> Result<Vec<String>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let scan_prefix = Key::new_kv(ns, prefix.unwrap_or(""));

            let results = txn.scan_prefix(&scan_prefix)?;

            Ok(results
                .into_iter()
                .filter_map(|(key, _)| key.user_key_string())
                .collect())
        })
    }

    /// List key-value pairs with optional prefix filter
    pub fn list_with_values(
        &self,
        run_id: &RunId,
        prefix: Option<&str>,
    ) -> Result<Vec<(String, Value)>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let scan_prefix = Key::new_kv(ns, prefix.unwrap_or(""));

            let results = txn.scan_prefix(&scan_prefix)?;

            Ok(results
                .into_iter()
                .filter_map(|(key, value)| {
                    key.user_key_string().map(|k| (k, value))
                })
                .collect())
        })
    }
}
```

### Tests

```rust
#[test]
fn test_list_all() {
    let (_temp, db, kv) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    kv.put(&run_id, "a", Value::I64(1)).unwrap();
    kv.put(&run_id, "b", Value::I64(2)).unwrap();
    kv.put(&run_id, "c", Value::I64(3)).unwrap();

    let keys = kv.list(&run_id, None).unwrap();
    assert_eq!(keys.len(), 3);
}

#[test]
fn test_list_with_prefix() {
    let (_temp, db, kv) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    kv.put(&run_id, "user:1", Value::I64(1)).unwrap();
    kv.put(&run_id, "user:2", Value::I64(2)).unwrap();
    kv.put(&run_id, "task:1", Value::I64(3)).unwrap();

    let user_keys = kv.list(&run_id, Some("user:")).unwrap();
    assert_eq!(user_keys.len(), 2);

    let task_keys = kv.list(&run_id, Some("task:")).unwrap();
    assert_eq!(task_keys.len(), 1);
}

#[test]
fn test_list_with_values() {
    let (_temp, db, kv) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    kv.put(&run_id, "key1", Value::String("val1".into())).unwrap();
    kv.put(&run_id, "key2", Value::String("val2".into())).unwrap();

    let pairs = kv.list_with_values(&run_id, None).unwrap();
    assert_eq!(pairs.len(), 2);
}
```

### Complete Story

```bash
./scripts/complete-story.sh 172
```

---

## Story #173: KVStoreExt Transaction Extension

**GitHub Issue**: [#173](https://github.com/anibjoshi/in-mem/issues/173)
**Estimated Time**: 3 hours
**Dependencies**: Stories #169-#172

### Start Story

```bash
/opt/homebrew/bin/gh issue view 173
./scripts/start-story.sh 14 173 kvstore-ext
```

### Implementation

Add to `crates/primitives/src/kv.rs`:

```rust
use crate::extensions::KVStoreExt;

impl KVStoreExt for TransactionContext {
    fn kv_get(&mut self, key: &str) -> Result<Option<Value>> {
        let storage_key = Key::new_kv(self.namespace().clone(), key);
        self.get(&storage_key)
    }

    fn kv_put(&mut self, key: &str, value: Value) -> Result<()> {
        let storage_key = Key::new_kv(self.namespace().clone(), key);
        self.put(storage_key, value)
    }

    fn kv_delete(&mut self, key: &str) -> Result<()> {
        let storage_key = Key::new_kv(self.namespace().clone(), key);
        self.delete(&storage_key)?;
        Ok(())
    }
}
```

Update `crates/primitives/src/lib.rs` to export KVStore:

```rust
pub mod kv;
pub use kv::KVStore;
```

### Tests

```rust
#[test]
fn test_kvstore_ext_in_transaction() {
    use crate::extensions::KVStoreExt;

    let (_temp, db, _kv) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    db.transaction(&run_id, |txn| {
        txn.kv_put("ext-key", Value::String("ext-value".into()))?;
        let val = txn.kv_get("ext-key")?;
        assert_eq!(val, Some(Value::String("ext-value".into())));
        Ok(())
    }).unwrap();
}
```

### Complete Story

```bash
./scripts/complete-story.sh 173
```

---

## Epic 14 Completion Checklist

### Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-primitives -- kv
~/.cargo/bin/cargo test --all
~/.cargo/bin/cargo clippy --all -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Verify Deliverables

- [ ] KVStore struct is stateless (only `Arc<Database>`)
- [ ] get/put/delete/list operations work
- [ ] put_with_ttl stores expiration metadata
- [ ] KVTransaction provides atomic multi-key operations
- [ ] KVStoreExt works in cross-primitive transactions
- [ ] Run isolation verified
- [ ] All tests pass

### Merge and Close

```bash
git checkout develop
git merge --no-ff epic-14-kvstore-primitive -m "Epic 14: KVStore Primitive

Complete:
- KVStore stateless facade
- Single-operation API (get, put, put_with_ttl, delete, exists)
- Multi-operation API (KVTransaction)
- List operations with prefix filtering
- KVStoreExt transaction extension

Stories: #169, #170, #171, #172, #173
"

/opt/homebrew/bin/gh issue close 160 --comment "Epic 14: KVStore Primitive - COMPLETE"
```

---

## Summary

Epic 14 implements the KVStore primitive - the simplest and most commonly used primitive. It establishes the stateless facade pattern that all other primitives will follow.
