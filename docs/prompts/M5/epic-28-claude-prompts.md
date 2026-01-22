# Epic 28: JsonStore Core - Implementation Prompts

**Epic Goal**: Implement stateless JsonStore facade over Database

**GitHub Issue**: [#258](https://github.com/anibjoshi/in-mem/issues/258)
**Status**: Ready after Epic 27 Story #230
**Dependencies**: Epic 27 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M5_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M5_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M5/EPIC_28_JSONSTORE_CORE.md`
3. **Prompt Header**: `docs/prompts/M5/M5_PROMPT_HEADER.md` for the 6 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## CRITICAL: STATELESS FACADE PATTERN

**This is the MOST CRITICAL epic. JsonStore MUST be stateless.**

```rust
// CORRECT: Stateless facade
#[derive(Clone)]
pub struct JsonStore {
    db: Arc<Database>,  // ONLY state - reference to database
}

// WRONG: Holding any internal state
pub struct JsonStore {
    db: Arc<Database>,
    cache: DashMap<Key, JsonDoc>,      // NEVER DO THIS
    documents: HashMap<JsonDocId, Doc>, // NEVER DO THIS
    lock: RwLock<()>,                  // NEVER DO THIS
}
```

---

## Epic 28 Overview

### Scope
- JsonStore struct (stateless facade)
- JsonDoc internal type (stored in ShardedStore)
- CRUD operations: create, get, set, delete_at_path, destroy
- Fast path reads via SnapshotView
- Serialization/deserialization

### Success Criteria
- [ ] JsonStore holds ONLY `Arc<Database>`
- [ ] Documents stored via Key::new_json() in ShardedStore
- [ ] create() stores new document
- [ ] get() uses fast path (SnapshotView)
- [ ] set() uses db.transaction() for mutations
- [ ] delete_at_path() removes values at paths
- [ ] destroy() removes entire document
- [ ] All operations respect run isolation

### Component Breakdown
- **Story #234 (GitHub #272)**: JsonStore Struct Definition - CRITICAL
- **Story #235 (GitHub #273)**: JsonDoc Internal Type
- **Story #236 (GitHub #274)**: Create Operation
- **Story #237 (GitHub #275)**: Fast Path Get Operation
- **Story #238 (GitHub #276)**: Set Operation (Mutation)
- **Story #239 (GitHub #277)**: Delete and Destroy Operations

---

## Dependency Graph

```
Story #272 (JsonStore struct) ──┬──> Story #273 (JsonDoc)
                                │         │
                                │         └──> Story #274 (create)
                                │                   │
                                │                   └──> Story #275 (get)
                                │                            │
                                └────────────────────────────┴──> Story #276 (set)
                                                                       │
                                                                       └──> Story #277 (delete/destroy)
```

---

## Story #272: JsonStore Struct Definition

**GitHub Issue**: [#272](https://github.com/anibjoshi/in-mem/issues/272)
**Estimated Time**: 2 hours
**Dependencies**: Epic 26, Epic 27
**CRITICAL**: This story defines the stateless pattern

### Start Story

```bash
gh issue view 272
./scripts/start-story.sh 28 272 jsonstore-struct
```

### Implementation

Create `crates/primitives/src/json_store.rs`:

```rust
//! JsonStore: JSON document storage primitive
//!
//! ## Design: STATELESS FACADE
//!
//! JsonStore holds ONLY `Arc<Database>`. No internal state, no caches,
//! no maps, no locks. All data lives in ShardedStore via Key::new_json().
//!
//! ## Run Isolation
//!
//! All operations are scoped to a run_id. Keys are prefixed with the
//! run's namespace, ensuring complete isolation between runs.

use std::sync::Arc;
use in_mem_engine::Database;
use in_mem_core::{Key, Namespace, RunId, TypeTag, JsonDocId, JsonPath, JsonValue, Result};

/// JSON document storage primitive
///
/// STATELESS FACADE over Database - all state lives in storage.
/// Multiple JsonStore instances on same Database are safe.
#[derive(Clone)]
pub struct JsonStore {
    db: Arc<Database>,  // ONLY state: reference to database
}

impl JsonStore {
    /// Create new JsonStore instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build key for JSON document
    fn key_for(&self, run_id: &RunId, doc_id: &JsonDocId) -> Key {
        let namespace = Namespace::for_run(run_id);
        Key::new_json(namespace, doc_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonstore_is_stateless() {
        // JsonStore should have size of single Arc pointer
        assert_eq!(
            std::mem::size_of::<JsonStore>(),
            std::mem::size_of::<Arc<Database>>()
        );
    }

    #[test]
    fn test_jsonstore_is_clone() {
        let db = Arc::new(Database::open_temp().unwrap());
        let store1 = JsonStore::new(db.clone());
        let store2 = store1.clone();
        assert!(Arc::ptr_eq(store1.database(), store2.database()));
    }

    #[test]
    fn test_jsonstore_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<JsonStore>();
    }
}
```

### VERIFICATION CHECKLIST

- [ ] JsonStore has ONLY one field: `db: Arc<Database>`
- [ ] No DashMap, HashMap, or any collection
- [ ] No RwLock, Mutex, or any lock
- [ ] No cache or memoization
- [ ] Size equals `Arc<Database>` size

### Complete Story

```bash
./scripts/complete-story.sh 272
```

---

## Story #273: JsonDoc Internal Type

**GitHub Issue**: [#273](https://github.com/anibjoshi/in-mem/issues/273)
**Estimated Time**: 2 hours
**Dependencies**: Story #272

### Start Story

```bash
gh issue view 273
./scripts/start-story.sh 28 273 json-doc
```

### Implementation

```rust
use serde::{Deserialize, Serialize};

/// Internal representation of a JSON document
///
/// Stored as serialized bytes in ShardedStore.
/// Version is used for optimistic concurrency control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDoc {
    /// Document unique identifier
    pub id: JsonDocId,
    /// The JSON value
    pub value: JsonValue,
    /// Version for conflict detection (incremented on each mutation)
    pub version: u64,
    /// Creation timestamp (millis since epoch)
    pub created_at: i64,
    /// Last modification timestamp
    pub updated_at: i64,
}

impl JsonDoc {
    pub fn new(id: JsonDocId, value: JsonValue) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        JsonDoc {
            id,
            value,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn increment_version(&mut self) {
        self.version += 1;
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
    }
}

impl JsonStore {
    /// Serialize document for storage
    fn serialize_doc(doc: &JsonDoc) -> Result<Vec<u8>> {
        rmp_serde::to_vec(doc).map_err(|e| Error::Serialization(e.to_string()))
    }

    /// Deserialize document from storage
    fn deserialize_doc(bytes: &[u8]) -> Result<JsonDoc> {
        rmp_serde::from_slice(bytes).map_err(|e| Error::Deserialization(e.to_string()))
    }
}
```

### Tests

```rust
#[test]
fn test_json_doc_roundtrip() {
    let doc = JsonDoc::new(JsonDocId::new(), JsonValue::from(42));
    let bytes = JsonStore::serialize_doc(&doc).unwrap();
    let recovered = JsonStore::deserialize_doc(&bytes).unwrap();
    assert_eq!(doc.id, recovered.id);
    assert_eq!(doc.value, recovered.value);
    assert_eq!(doc.version, recovered.version);
}
```

### Complete Story

```bash
./scripts/complete-story.sh 273
```

---

## Story #274: Create Operation

**GitHub Issue**: [#274](https://github.com/anibjoshi/in-mem/issues/274)
**Estimated Time**: 2 hours
**Dependencies**: Story #273

### Start Story

```bash
gh issue view 274
./scripts/start-story.sh 28 274 json-create
```

### Implementation

```rust
impl JsonStore {
    /// Create a new JSON document
    ///
    /// Returns the document ID on success.
    /// Fails if document with same ID already exists.
    pub fn create(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        value: JsonValue,
    ) -> Result<u64> {
        // Validate the value
        validate_json_value(&value)?;

        let key = self.key_for(run_id, doc_id);
        let doc = JsonDoc::new(*doc_id, value);

        self.db.transaction(*run_id, |txn| {
            // Check if document already exists
            if txn.get(&key)?.is_some() {
                return Err(Error::AlreadyExists(format!("JSON document {}", doc_id)));
            }

            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), Value::Bytes(serialized))?;
            Ok(doc.version)
        })
    }
}
```

### Tests

```rust
#[test]
fn test_create_document() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let version = json.create(&run_id, &doc_id, JsonValue::from(42)).unwrap();
    assert_eq!(version, 1);
}

#[test]
fn test_create_duplicate_fails() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::from(1)).unwrap();
    let result = json.create(&run_id, &doc_id, JsonValue::from(2));
    assert!(result.is_err());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 274
```

---

## Story #275: Fast Path Get Operation

**GitHub Issue**: [#275](https://github.com/anibjoshi/in-mem/issues/275)
**Estimated Time**: 3 hours
**Dependencies**: Story #274
**CRITICAL**: Must use SnapshotView, NOT transaction

### Start Story

```bash
gh issue view 275
./scripts/start-story.sh 28 275 json-get
```

### Implementation

```rust
impl JsonStore {
    /// Get value at path in a document (FAST PATH)
    ///
    /// Uses direct snapshot read, bypassing transaction overhead.
    /// Observationally equivalent to transaction-based read.
    pub fn get(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<JsonValue>> {
        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key) {
            Some(vv) => {
                let doc = Self::deserialize_doc(vv.value.as_bytes()?)?;
                Ok(get_at_path(&doc.value, path).cloned())
            }
            None => Ok(None),
        }
    }

    /// Get entire document (FAST PATH)
    pub fn get_doc(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
    ) -> Result<Option<JsonDoc>> {
        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key) {
            Some(vv) => {
                let doc = Self::deserialize_doc(vv.value.as_bytes()?)?;
                Ok(Some(doc))
            }
            None => Ok(None),
        }
    }

    /// Check if document exists (FAST PATH)
    pub fn exists(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool> {
        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);
        Ok(snapshot.get(&key).is_some())
    }
}
```

### VERIFICATION CHECKLIST

- [ ] Uses `create_snapshot()` NOT `transaction()`
- [ ] No transaction allocation
- [ ] No read-set recording
- [ ] No commit validation

### Tests

```rust
#[test]
fn test_get_at_path() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let mut obj = IndexMap::new();
    obj.insert("name".to_string(), JsonValue::from("test"));
    obj.insert("count".to_string(), JsonValue::from(42));
    json.create(&run_id, &doc_id, JsonValue::Object(obj)).unwrap();

    let name = json.get(&run_id, &doc_id, &JsonPath::parse("name").unwrap()).unwrap();
    assert_eq!(name.and_then(|v| v.as_str().map(String::from)), Some("test".to_string()));

    let count = json.get(&run_id, &doc_id, &JsonPath::parse("count").unwrap()).unwrap();
    assert_eq!(count.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_get_missing_document() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let result = json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
    assert!(result.is_none());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 275
```

---

## Story #276: Set Operation (Mutation)

**GitHub Issue**: [#276](https://github.com/anibjoshi/in-mem/issues/276)
**Estimated Time**: 3 hours
**Dependencies**: Story #275

### Start Story

```bash
gh issue view 276
./scripts/start-story.sh 28 276 json-set
```

### Implementation

```rust
impl JsonStore {
    /// Set value at path in a document
    ///
    /// Uses transaction for atomic read-modify-write.
    /// Increments document version.
    pub fn set(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<u64> {
        validate_json_value(&value)?;
        validate_path(path)?;

        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            // Load existing document
            let vv = txn.get(&key)?.ok_or(Error::NotFound(format!("JSON document {}", doc_id)))?;
            let mut doc = Self::deserialize_doc(vv.value.as_bytes()?)?;

            // Apply mutation
            set_at_path(&mut doc.value, path, value)?;
            doc.increment_version();

            // Validate result
            validate_json_value(&doc.value)?;

            // Store updated document
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), Value::Bytes(serialized))?;

            Ok(doc.version)
        })
    }
}
```

### Tests

```rust
#[test]
fn test_set_at_path() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    let v2 = json.set(&run_id, &doc_id, &JsonPath::parse("a").unwrap(), JsonValue::from(1)).unwrap();
    assert_eq!(v2, 2);

    let result = json.get(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap();
    assert_eq!(result.and_then(|v| v.as_i64()), Some(1));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 276
```

---

## Story #277: Delete and Destroy Operations

**GitHub Issue**: [#277](https://github.com/anibjoshi/in-mem/issues/277)
**Estimated Time**: 2 hours
**Dependencies**: Story #276

### Start Story

```bash
gh issue view 277
./scripts/start-story.sh 28 277 json-delete-destroy
```

### Implementation

```rust
impl JsonStore {
    /// Delete value at path in a document
    pub fn delete_at_path(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<u64> {
        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            let vv = txn.get(&key)?.ok_or(Error::NotFound(format!("JSON document {}", doc_id)))?;
            let mut doc = Self::deserialize_doc(vv.value.as_bytes()?)?;

            delete_at_path(&mut doc.value, path)?;
            doc.increment_version();

            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), Value::Bytes(serialized))?;

            Ok(doc.version)
        })
    }

    /// Destroy entire document
    pub fn destroy(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool> {
        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            let existed = txn.get(&key)?.is_some();
            if existed {
                txn.delete(&key)?;
            }
            Ok(existed)
        })
    }
}
```

### Tests

```rust
#[test]
fn test_delete_at_path() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let mut obj = IndexMap::new();
    obj.insert("a".to_string(), JsonValue::from(1));
    obj.insert("b".to_string(), JsonValue::from(2));
    json.create(&run_id, &doc_id, JsonValue::Object(obj)).unwrap();

    json.delete_at_path(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap();

    assert!(json.get(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap().is_none());
    assert!(json.get(&run_id, &doc_id, &JsonPath::parse("b").unwrap()).unwrap().is_some());
}

#[test]
fn test_destroy_document() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::from(42)).unwrap();
    assert!(json.exists(&run_id, &doc_id).unwrap());

    json.destroy(&run_id, &doc_id).unwrap();
    assert!(!json.exists(&run_id, &doc_id).unwrap());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 277
```

---

## Epic 28 Completion Checklist

### CRITICAL VERIFICATION

Before merging, verify JsonStore is truly stateless:

```rust
// This assertion MUST pass
assert_eq!(
    std::mem::size_of::<JsonStore>(),
    std::mem::size_of::<Arc<Database>>()
);
```

### Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-primitives -- json
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
```

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-28-jsonstore-core -m "Epic 28: JsonStore Core complete

CRITICAL: JsonStore is STATELESS - holds only Arc<Database>

Delivered:
- JsonStore stateless facade
- JsonDoc internal type with versioning
- create() operation
- get() with fast path (SnapshotView)
- set() with transaction
- delete_at_path() and destroy()

Stories: #272, #273, #274, #275, #276, #277
"
git push origin develop
gh issue close 258 --comment "Epic 28: JsonStore Core - COMPLETE"
```
