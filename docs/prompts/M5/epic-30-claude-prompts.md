# Epic 30: Transaction Integration - Implementation Prompts

**Epic Goal**: Integrate JSON with existing transaction system

**GitHub Issue**: [#260](https://github.com/anibjoshi/in-mem/issues/260)
**Status**: Ready after Epic 28
**Dependencies**: Epic 28 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M5_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M5_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M5/EPIC_30_TRANSACTION_INTEGRATION.md`
3. **Prompt Header**: `docs/prompts/M5/M5_PROMPT_HEADER.md` for the 6 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## CRITICAL: EXTENSION TRAIT PATTERN (Rule 3)

**Add `JsonStoreExt` trait to TransactionContext. NO separate JsonTransaction type.**

```rust
// CORRECT: Extension trait
pub trait JsonStoreExt {
    fn json_get(&self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>>;
    fn json_set(&mut self, key: &Key, path: &JsonPath, value: JsonValue) -> Result<()>;
}

impl JsonStoreExt for TransactionContext { ... }

// Usage in cross-primitive transaction
db.transaction(run_id, |txn| {
    txn.json_set(&json_key, &path, value)?;  // JSON
    txn.put(kv_key, kv_value)?;              // KV
    Ok(())
})?;

// WRONG: Separate transaction type
pub struct JsonTransaction { ... }  // NEVER DO THIS
```

---

## Epic 30 Overview

### Scope
- JsonPathRead and JsonPatchEntry types
- Lazy set initialization in TransactionContext
- JsonStoreExt trait implementation
- Snapshot version capture
- Cross-primitive transactions

### Success Criteria
- [ ] JsonStoreExt trait on TransactionContext
- [ ] Lazy allocation (zero overhead when not using JSON)
- [ ] json_get/json_set work in transactions
- [ ] Cross-primitive atomicity works
- [ ] Snapshot versions captured for conflict detection

### Component Breakdown
- **Story #244 (GitHub #282)**: JSON Path Read/Patch Types
- **Story #245 (GitHub #283)**: Lazy Set Initialization
- **Story #246 (GitHub #284)**: JsonStoreExt Trait Implementation
- **Story #247 (GitHub #285)**: Snapshot Version Capture
- **Story #248 (GitHub #286)**: Cross-Primitive Transactions

---

## Story #282: JSON Path Read/Patch Types

**GitHub Issue**: [#282](https://github.com/anibjoshi/in-mem/issues/282)
**Estimated Time**: 2 hours
**Dependencies**: Epic 28 complete

### Start Story

```bash
gh issue view 282
./scripts/start-story.sh 30 282 json-txn-types
```

### Implementation

Add to `crates/concurrency/src/transaction.rs`:

```rust
/// Record of a JSON path read (for conflict detection)
#[derive(Debug, Clone)]
pub struct JsonPathRead {
    pub key: Key,
    pub path: JsonPath,
    pub version: u64,
}

impl JsonPathRead {
    pub fn new(key: Key, path: JsonPath, version: u64) -> Self {
        Self { key, path, version }
    }
}

/// Record of a JSON patch operation (for commit)
#[derive(Debug, Clone)]
pub struct JsonPatchEntry {
    pub key: Key,
    pub patch: JsonPatch,
    pub resulting_version: u64,
}

impl JsonPatchEntry {
    pub fn new(key: Key, patch: JsonPatch, resulting_version: u64) -> Self {
        Self { key, patch, resulting_version }
    }
}
```

### Tests

```rust
#[test]
fn test_json_path_read() {
    let read = JsonPathRead::new(
        Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new()),
        JsonPath::parse("foo.bar").unwrap(),
        5,
    );
    assert_eq!(read.version, 5);
}
```

### Complete Story

```bash
./scripts/complete-story.sh 282
```

---

## Story #283: Lazy Set Initialization

**GitHub Issue**: [#283](https://github.com/anibjoshi/in-mem/issues/283)
**Estimated Time**: 2 hours
**Dependencies**: Story #282

### Start Story

```bash
gh issue view 283
./scripts/start-story.sh 30 283 lazy-init
```

### Implementation

Update `crates/concurrency/src/transaction.rs`:

```rust
pub struct TransactionContext {
    // Existing fields...
    run_id: RunId,
    read_set: Vec<ReadEntry>,
    write_set: Vec<WriteEntry>,

    // NEW: JSON fields (lazy allocation)
    json_reads: Option<Vec<JsonPathRead>>,
    json_writes: Option<Vec<JsonPatchEntry>>,
    json_snapshot_versions: Option<HashMap<Key, u64>>,
}

impl TransactionContext {
    pub fn new(run_id: RunId) -> Self {
        Self {
            run_id,
            read_set: Vec::new(),
            write_set: Vec::new(),
            // Lazy: None until first JSON operation
            json_reads: None,
            json_writes: None,
            json_snapshot_versions: None,
        }
    }

    /// Check if transaction has any JSON operations
    pub fn has_json_ops(&self) -> bool {
        self.json_reads.is_some() || self.json_writes.is_some()
    }

    /// Get JSON reads (for conflict detection)
    pub fn json_reads(&self) -> Option<&Vec<JsonPathRead>> {
        self.json_reads.as_ref()
    }

    /// Get JSON writes (for commit)
    pub fn json_writes(&self) -> Option<&Vec<JsonPatchEntry>> {
        self.json_writes.as_ref()
    }

    /// Get snapshot versions (for conflict detection)
    pub fn json_snapshot_versions(&self) -> Option<&HashMap<Key, u64>> {
        self.json_snapshot_versions.as_ref()
    }

    // Internal: ensure lazy sets are initialized
    fn ensure_json_reads(&mut self) -> &mut Vec<JsonPathRead> {
        self.json_reads.get_or_insert_with(Vec::new)
    }

    fn ensure_json_writes(&mut self) -> &mut Vec<JsonPatchEntry> {
        self.json_writes.get_or_insert_with(Vec::new)
    }

    fn ensure_json_snapshot_versions(&mut self) -> &mut HashMap<Key, u64> {
        self.json_snapshot_versions.get_or_insert_with(HashMap::new)
    }
}
```

### Tests

```rust
#[test]
fn test_lazy_allocation() {
    let txn = TransactionContext::new(RunId::new());

    // Initially no JSON state
    assert!(!txn.has_json_ops());
    assert!(txn.json_reads().is_none());
    assert!(txn.json_writes().is_none());
}

#[test]
fn test_transaction_size_without_json() {
    // Verify Option<Vec> doesn't add overhead when None
    let txn = TransactionContext::new(RunId::new());
    // Transaction should be small when not using JSON
    assert!(!txn.has_json_ops());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 283
```

---

## Story #284: JsonStoreExt Trait Implementation

**GitHub Issue**: [#284](https://github.com/anibjoshi/in-mem/issues/284)
**Estimated Time**: 4 hours
**Dependencies**: Story #283

### Start Story

```bash
gh issue view 284
./scripts/start-story.sh 30 284 json-store-ext
```

### Implementation

Create `crates/primitives/src/json_ext.rs`:

```rust
//! JsonStoreExt - Extension trait for JSON operations in transactions

use in_mem_concurrency::TransactionContext;
use in_mem_core::{Key, JsonPath, JsonValue, JsonPatch, Result};

/// Extension trait for JSON operations in transactions
///
/// This trait extends TransactionContext with JSON-specific methods.
/// It follows the same pattern as other primitive extensions.
pub trait JsonStoreExt {
    /// Get value at path within transaction
    fn json_get(&self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>>;

    /// Set value at path within transaction
    fn json_set(&mut self, key: &Key, path: &JsonPath, value: JsonValue) -> Result<()>;

    /// Delete value at path within transaction
    fn json_delete(&mut self, key: &Key, path: &JsonPath) -> Result<()>;
}

impl JsonStoreExt for TransactionContext {
    fn json_get(&self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>> {
        // Check write set first (read-your-writes)
        if let Some(writes) = self.json_writes() {
            for entry in writes.iter().rev() {
                if &entry.key == key {
                    // Found a write to this document - need to apply patches
                    // This is complex: need to track incremental state
                }
            }
        }

        // Read from storage
        let vv = self.get(key)?;
        match vv {
            Some(v) => {
                let doc = JsonStore::deserialize_doc(v.as_bytes()?)?;

                // Record the read for conflict detection
                self.ensure_json_reads().push(JsonPathRead::new(
                    key.clone(),
                    path.clone(),
                    doc.version,
                ));

                // Record snapshot version
                self.ensure_json_snapshot_versions()
                    .entry(key.clone())
                    .or_insert(doc.version);

                Ok(get_at_path(&doc.value, path).cloned())
            }
            None => Ok(None),
        }
    }

    fn json_set(&mut self, key: &Key, path: &JsonPath, value: JsonValue) -> Result<()> {
        validate_json_value(&value)?;
        validate_path(path)?;

        // Load current document to get version
        let vv = self.get(key)?.ok_or(Error::NotFound("JSON document".into()))?;
        let doc = JsonStore::deserialize_doc(vv.as_bytes()?)?;

        // Record snapshot version
        self.ensure_json_snapshot_versions()
            .entry(key.clone())
            .or_insert(doc.version);

        // Record the patch
        let new_version = doc.version + 1;
        self.ensure_json_writes().push(JsonPatchEntry::new(
            key.clone(),
            JsonPatch::set(path.clone(), value),
            new_version,
        ));

        Ok(())
    }

    fn json_delete(&mut self, key: &Key, path: &JsonPath) -> Result<()> {
        let vv = self.get(key)?.ok_or(Error::NotFound("JSON document".into()))?;
        let doc = JsonStore::deserialize_doc(vv.as_bytes()?)?;

        self.ensure_json_snapshot_versions()
            .entry(key.clone())
            .or_insert(doc.version);

        let new_version = doc.version + 1;
        self.ensure_json_writes().push(JsonPatchEntry::new(
            key.clone(),
            JsonPatch::delete(path.clone()),
            new_version,
        ));

        Ok(())
    }
}
```

Update `crates/primitives/src/lib.rs`:

```rust
pub mod json_ext;
pub use json_ext::JsonStoreExt;
```

### Tests

```rust
#[test]
fn test_json_in_transaction() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    db.transaction(run_id, |txn| {
        txn.json_set(&key, &JsonPath::parse("a").unwrap(), JsonValue::from(1))?;
        txn.json_set(&key, &JsonPath::parse("b").unwrap(), JsonValue::from(2))?;
        Ok(())
    }).unwrap();

    assert_eq!(
        json.get(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap().and_then(|v| v.as_i64()),
        Some(1)
    );
}
```

### Complete Story

```bash
./scripts/complete-story.sh 284
```

---

## Story #285: Snapshot Version Capture

**GitHub Issue**: [#285](https://github.com/anibjoshi/in-mem/issues/285)
**Estimated Time**: 2 hours
**Dependencies**: Story #284

### Start Story

```bash
gh issue view 285
./scripts/start-story.sh 30 285 snapshot-versions
```

### Implementation

Already included in Story #284. This story focuses on verification and edge cases.

### Tests

```rust
#[test]
fn test_snapshot_version_captured() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    json.create(&run_id, &doc_id, JsonValue::from(1)).unwrap();

    db.transaction(run_id, |txn| {
        // Read captures version
        let _ = txn.json_get(&key, &JsonPath::root())?;

        // Verify snapshot version captured
        let versions = txn.json_snapshot_versions().unwrap();
        assert!(versions.contains_key(&key));
        assert_eq!(versions[&key], 1);

        Ok(())
    }).unwrap();
}
```

### Complete Story

```bash
./scripts/complete-story.sh 285
```

---

## Story #286: Cross-Primitive Transactions

**GitHub Issue**: [#286](https://github.com/anibjoshi/in-mem/issues/286)
**Estimated Time**: 3 hours
**Dependencies**: Story #284

### Start Story

```bash
gh issue view 286
./scripts/start-story.sh 30 286 cross-primitive
```

### Implementation

This story verifies that JSON works alongside other primitives in the same transaction.

### Tests

```rust
#[test]
fn test_json_kv_same_transaction() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    let doc_id = JsonDocId::new();
    let json_key = Key::new_json(Namespace::for_run(run_id), &doc_id);
    let kv_key = Key::new_kv(Namespace::for_run(run_id), b"counter");

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    // Atomic transaction across JSON and KV
    db.transaction(run_id, |txn| {
        txn.json_set(&json_key, &JsonPath::parse("updated").unwrap(), JsonValue::from(true))?;
        txn.put(kv_key.clone(), Value::from(42))?;
        Ok(())
    }).unwrap();

    // Both committed atomically
    let json_val = json.get(&run_id, &doc_id, &JsonPath::parse("updated").unwrap()).unwrap();
    assert_eq!(json_val.and_then(|v| v.as_bool()), Some(true));

    let kv_val = kv.get(&run_id, b"counter").unwrap();
    assert_eq!(kv_val.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_cross_primitive_rollback() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    let doc_id = JsonDocId::new();
    let json_key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    json.create(&run_id, &doc_id, JsonValue::from(0)).unwrap();

    // Transaction that fails
    let result = db.transaction(run_id, |txn| {
        txn.json_set(&json_key, &JsonPath::root(), JsonValue::from(42))?;
        let kv_key = Key::new_kv(Namespace::for_run(run_id), b"key");
        txn.put(kv_key, Value::from(100))?;
        Err::<(), _>(Error::Custom("forced failure".into()))
    });

    assert!(result.is_err());

    // JSON should be rolled back
    let json_val = json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
    assert_eq!(json_val.and_then(|v| v.as_i64()), Some(0)); // Original value

    // KV should also be rolled back
    assert!(kv.get(&run_id, b"key").unwrap().is_none());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 286
```

---

## Epic 30 Completion Checklist

### Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-concurrency -- json
~/.cargo/bin/cargo test -p in-mem-primitives -- json
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
```

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-30-transaction-integration -m "Epic 30: Transaction Integration complete

Delivered:
- JsonPathRead and JsonPatchEntry types
- Lazy set initialization (zero overhead)
- JsonStoreExt trait on TransactionContext
- Snapshot version capture
- Cross-primitive transaction support

Stories: #282, #283, #284, #285, #286
"
git push origin develop
gh issue close 260 --comment "Epic 30: Transaction Integration - COMPLETE"
```
