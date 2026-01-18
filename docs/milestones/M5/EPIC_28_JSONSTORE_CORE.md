# Epic 28: JsonStore Core

**Goal**: Implement the JsonStore facade with full API

**Dependencies**: Epic 27 complete

**GitHub Issue**: #258

---

## Scope

- JsonDoc internal structure
- JsonStore struct definition (STATELESS FACADE)
- Document create/delete operations
- Path-based get/set/delete operations
- Exists/list operations
- MessagePack serialization

---

## Architectural Integration Rules

**CRITICAL**: JsonStore MUST follow the stateless facade pattern like all other primitives:

1. **JsonStore holds ONLY `Arc<Database>`** - no internal maps, locks, or state
2. **Uses unified ShardedStore** via `Key::new_json()` - no separate DashMap storage
3. **Uses SnapshotView for fast path reads** - direct snapshot access
4. **Uses db.transaction() for mutations** - unified transaction system

---

## User Stories

| Story | Description | Priority | GitHub Issue |
|-------|-------------|----------|--------------|
| #234 | JsonDoc Internal Structure | FOUNDATION | #272 |
| #235 | JsonStore Struct Definition (Stateless Facade) | FOUNDATION | #273 |
| #236 | Document Create/Delete | CRITICAL | #274 |
| #237 | Document Get/Set/Delete at Path | CRITICAL | #275 |
| #238 | Document Exists/List | HIGH | #276 |
| #239 | Serialization | HIGH | #277 |

---

## Story #234: JsonDoc Internal Structure

**File**: `crates/core/src/json_types.rs`

**Deliverable**: JsonDoc struct with version tracking

### Implementation

```rust
use std::time::SystemTime;

/// Internal document representation
///
/// DESIGN: Document-level versioning (single version for entire doc)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDoc {
    /// Document unique identifier
    pub id: JsonDocId,
    /// The JSON value (root of document)
    pub value: JsonValue,
    /// Document version (increments on any change)
    pub version: u64,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Last modification timestamp
    pub updated_at: SystemTime,
}

impl JsonDoc {
    /// Create a new document with initial value
    pub fn new(id: JsonDocId, value: JsonValue) -> Self {
        let now = SystemTime::now();
        JsonDoc {
            id,
            value,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    /// Increment version and update timestamp
    pub fn touch(&mut self) {
        self.version += 1;
        self.updated_at = SystemTime::now();
    }

    /// Get document size estimate in bytes
    pub fn size_estimate(&self) -> usize {
        // Rough estimate: serialize and check length
        // In production, use a more efficient method
        std::mem::size_of::<Self>()
    }
}
```

### Acceptance Criteria

- [ ] JsonDoc stores all required fields
- [ ] new() initializes version to 1
- [ ] touch() increments version
- [ ] touch() updates updated_at

### Testing

```rust
#[test]
fn test_json_doc_new() {
    let id = JsonDocId::new();
    let value = JsonValue::Object(IndexMap::new());
    let doc = JsonDoc::new(id, value);

    assert_eq!(doc.id, id);
    assert_eq!(doc.version, 1);
    assert!(doc.created_at <= doc.updated_at);
}

#[test]
fn test_json_doc_touch() {
    let id = JsonDocId::new();
    let value = JsonValue::Object(IndexMap::new());
    let mut doc = JsonDoc::new(id, value);

    let old_version = doc.version;
    let old_updated = doc.updated_at;

    std::thread::sleep(std::time::Duration::from_millis(1));
    doc.touch();

    assert_eq!(doc.version, old_version + 1);
    assert!(doc.updated_at > old_updated);
}
```

---

## Story #235: JsonStore Struct Definition (Stateless Facade)

**File**: `crates/primitives/src/json_store.rs` (NEW)

**Deliverable**: JsonStore as a STATELESS FACADE (like all other primitives)

**CRITICAL**: JsonStore must follow the same pattern as KVStore, EventLog, StateCell, Trace, RunIndex.

### Implementation

```rust
use in_mem_core::error::Result;
use in_mem_core::json_types::{
    JsonDoc, JsonDocId, JsonPath, JsonValue, JsonPatch,
    get_at_path, set_at_path, delete_at_path,
    validate_json_value, validate_path,
};
use in_mem_core::types::{Key, Namespace, RunId};
use in_mem_core::value::Value;
use in_mem_engine::Database;
use std::sync::Arc;

/// JSON document storage primitive
///
/// STATELESS FACADE over Database - all state lives in unified ShardedStore.
/// Multiple JsonStore instances on same Database are safe.
///
/// # Design
///
/// JsonStore does NOT own storage. It is a facade that:
/// - Uses Arc<Database> for all operations
/// - Stores documents via Key::new_json() in ShardedStore
/// - Uses SnapshotView for fast path reads
/// - Participates in cross-primitive transactions
///
/// # Example
///
/// ```ignore
/// let db = Arc::new(Database::open("/path/to/data")?);
/// let json = JsonStore::new(db);
/// let run_id = RunId::new();
/// let doc_id = JsonDocId::new();
///
/// // Simple operations
/// json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new()))?;
/// let value = json.get(&run_id, &doc_id, &JsonPath::root())?;
/// json.set(&run_id, &doc_id, &JsonPath::parse("foo")?, JsonValue::from(42))?;
/// ```
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

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(*run_id)
    }

    /// Build key for JSON document
    fn key_for(&self, run_id: &RunId, doc_id: &JsonDocId) -> Key {
        Key::new_json(self.namespace_for_run(run_id), doc_id)
    }
}
```

### Acceptance Criteria

- [ ] JsonStore holds ONLY `Arc<Database>` (stateless facade)
- [ ] Uses Key::new_json() for storage keys
- [ ] Clone is cheap (Arc clone only)
- [ ] No DashMap, no internal storage
- [ ] No locks, no internal state

### Testing

```rust
#[test]
fn test_json_store_is_stateless() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json1 = JsonStore::new(db.clone());
    let json2 = JsonStore::new(db.clone());

    // Both should see same data
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json1.create(&run_id, &doc_id, JsonValue::from(42)).unwrap();
    let value = json2.get(&run_id, &doc_id, &JsonPath::root()).unwrap();

    assert_eq!(value.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_json_store_clone_is_cheap() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);

    // Clone should be instant (just Arc clone)
    let cloned = json.clone();
    assert!(Arc::ptr_eq(json.database(), cloned.database()));
}
```

---

## Story #236: Document Create/Delete

**File**: `crates/primitives/src/json_store.rs`

**Deliverable**: Document creation and deletion operations (using unified storage)

### Implementation

```rust
impl JsonStore {
    /// Create a new document
    ///
    /// Uses implicit transaction for atomic create.
    pub fn create(&self, run_id: &RunId, doc_id: &JsonDocId, value: JsonValue) -> Result<()> {
        validate_json_value(&value)?;

        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            // Check doesn't already exist
            if txn.get(&key)?.is_some() {
                return Err(Error::AlreadyExists(format!("Document {}", doc_id)));
            }

            let doc = JsonDoc::new(*doc_id, value);
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key, serialized)
        })
    }

    /// Delete a document entirely
    ///
    /// Uses implicit transaction for atomic delete.
    pub fn delete_doc(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            if txn.get(&key)?.is_some() {
                txn.delete(key)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    /// Create document if it doesn't exist, or return existing
    pub fn get_or_create(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        default_value: impl FnOnce() -> JsonValue,
    ) -> Result<JsonValue> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            if let Some(existing) = txn.get(&key)? {
                let doc = Self::deserialize_doc(&existing)?;
                return Ok(doc.value);
            }

            let value = default_value();
            validate_json_value(&value)?;

            let doc = JsonDoc::new(*doc_id, value.clone());
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key, serialized)?;
            Ok(value)
        })
    }
}
```

### Acceptance Criteria

- [ ] create() validates document before storing
- [ ] create() uses unified Key::new_json()
- [ ] create() fails if document exists (via transaction)
- [ ] delete_doc() uses unified storage delete
- [ ] delete_doc() returns bool (true if deleted)
- [ ] get_or_create() is atomic

### Testing

```rust
#[test]
fn test_create_document() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let value = JsonValue::Object(IndexMap::new());
    json.create(&run_id, &doc_id, value).unwrap();

    assert!(json.exists(&run_id, &doc_id).unwrap());
}

#[test]
fn test_create_fails_if_exists() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::from(1)).unwrap();
    let result = json.create(&run_id, &doc_id, JsonValue::from(2));

    assert!(result.is_err());
}

#[test]
fn test_delete_document() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::from(42)).unwrap();
    assert!(json.exists(&run_id, &doc_id).unwrap());

    let deleted = json.delete_doc(&run_id, &doc_id).unwrap();
    assert!(deleted);
    assert!(!json.exists(&run_id, &doc_id).unwrap());
}

#[test]
fn test_delete_nonexistent() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let deleted = json.delete_doc(&run_id, &doc_id).unwrap();
    assert!(!deleted);
}
```

---

## Story #237: Document Get/Set/Delete at Path

**File**: `crates/primitives/src/json_store.rs`

**Deliverable**: Path-based document operations (using unified storage)

### Implementation

```rust
impl JsonStore {
    // ========== Fast Path (Implicit Transactions) ==========

    /// Get value at path in document (FAST PATH)
    ///
    /// Uses SnapshotView directly for read-only access.
    /// Bypasses full transaction overhead:
    /// - Direct snapshot read
    /// - No transaction object allocation
    /// - No read-set recording
    pub fn get(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc = Self::deserialize_doc(&vv.value)?;
                Ok(get_at_path(&doc.value, path).cloned())
            }
            None => Ok(None),
        }
    }

    /// Get document version (FAST PATH)
    pub fn get_version(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<Option<u64>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc = Self::deserialize_doc(&vv.value)?;
                Ok(Some(doc.version))
            }
            None => Ok(None),
        }
    }

    /// Get full document (FAST PATH)
    pub fn get_doc(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<Option<JsonDoc>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc = Self::deserialize_doc(&vv.value)?;
                Ok(Some(doc))
            }
            None => Ok(None),
        }
    }

    // ========== Mutations (Implicit Transactions) ==========

    /// Set value at path in document
    ///
    /// Uses implicit transaction for atomic update.
    pub fn set(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<u64> {
        validate_json_value(&value)?;
        validate_path(path)?;

        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            // Get existing doc
            let mut doc = match txn.get(&key)? {
                Some(v) => Self::deserialize_doc(&v)?,
                None => return Err(Error::NotFound(format!("Document {}", doc_id))),
            };

            // Apply patch
            set_at_path(&mut doc.value, path, value)?;
            doc.touch();

            let new_version = doc.version;
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key, serialized)?;
            Ok(new_version)
        })
    }

    /// Delete value at path in document
    ///
    /// Uses implicit transaction for atomic update.
    pub fn delete_at_path(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            // Get existing doc
            let mut doc = match txn.get(&key)? {
                Some(v) => Self::deserialize_doc(&v)?,
                None => return Err(Error::NotFound(format!("Document {}", doc_id))),
            };

            // Apply delete
            let deleted = delete_at_path(&mut doc.value, path)?;
            doc.touch();

            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key, serialized)?;
            Ok(deleted)
        })
    }

    /// Apply multiple patches atomically
    pub fn apply_patches(&self, run_id: &RunId, doc_id: &JsonDocId, patches: Vec<JsonPatch>) -> Result<u64> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            let mut doc = match txn.get(&key)? {
                Some(v) => Self::deserialize_doc(&v)?,
                None => return Err(Error::NotFound(format!("Document {}", doc_id))),
            };

            for patch in patches {
                match patch {
                    JsonPatch::Set { path, value } => {
                        validate_json_value(&value)?;
                        validate_path(&path)?;
                        set_at_path(&mut doc.value, &path, value)?;
                    }
                    JsonPatch::Delete { path } => {
                        delete_at_path(&mut doc.value, &path)?;
                    }
                }
            }

            doc.touch();
            let new_version = doc.version;
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key, serialized)?;
            Ok(new_version)
        })
    }
}
```

### Acceptance Criteria

- [ ] get() uses SnapshotView directly (fast path)
- [ ] get() returns cloned value at path
- [ ] get() returns None for non-existent paths
- [ ] set() validates value and path
- [ ] set() increments version
- [ ] set() uses db.transaction()
- [ ] delete_at_path() returns deleted value
- [ ] apply_patches() is atomic

### Testing

```rust
#[test]
fn test_get_at_path() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let mut obj = IndexMap::new();
    obj.insert("foo".to_string(), JsonValue::from(42));
    json.create(&run_id, &doc_id, JsonValue::Object(obj)).unwrap();

    let value = json.get(&run_id, &doc_id, &JsonPath::parse("foo").unwrap()).unwrap();
    assert_eq!(value.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_set_at_path() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    let version = json.set(&run_id, &doc_id, &JsonPath::parse("foo").unwrap(), JsonValue::from(42)).unwrap();
    assert_eq!(version, 2); // Version incremented

    let value = json.get(&run_id, &doc_id, &JsonPath::parse("foo").unwrap()).unwrap();
    assert_eq!(value.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_delete_at_path() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let mut obj = IndexMap::new();
    obj.insert("foo".to_string(), JsonValue::from(42));
    json.create(&run_id, &doc_id, JsonValue::Object(obj)).unwrap();

    let deleted = json.delete_at_path(&run_id, &doc_id, &JsonPath::parse("foo").unwrap()).unwrap();
    assert_eq!(deleted.and_then(|v| v.as_i64()), Some(42));

    let value = json.get(&run_id, &doc_id, &JsonPath::parse("foo").unwrap()).unwrap();
    assert!(value.is_none());
}

#[test]
fn test_apply_patches() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    let patches = vec![
        JsonPatch::set(JsonPath::parse("a").unwrap(), 1),
        JsonPatch::set(JsonPath::parse("b").unwrap(), 2),
        JsonPatch::set(JsonPath::parse("c").unwrap(), 3),
    ];

    let version = json.apply_patches(&run_id, &doc_id, patches).unwrap();
    assert_eq!(version, 2); // Only incremented once

    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(1));
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("b").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(2));
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("c").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(3));
}
```

---

## Story #238: Document Exists/List

**File**: `crates/primitives/src/json_store.rs`

**Deliverable**: Document existence and listing operations

### Implementation

```rust
impl JsonStore {
    /// Check if document exists (FAST PATH)
    pub fn exists(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);
        Ok(snapshot.get(&key)?.is_some())
    }

    /// List all document IDs in a run
    pub fn list(&self, run_id: &RunId) -> Result<Vec<JsonDocId>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let prefix = Key::new_json_prefix(self.namespace_for_run(run_id));

        let entries = snapshot.scan_prefix(&prefix)?;
        let doc_ids = entries
            .into_iter()
            .filter_map(|(key, _)| {
                // Extract doc_id from key user_key bytes
                JsonDocId::try_from_bytes(key.user_key())
            })
            .collect();

        Ok(doc_ids)
    }

    /// Count documents in a run
    pub fn count(&self, run_id: &RunId) -> Result<usize> {
        self.list(run_id).map(|ids| ids.len())
    }

    /// Iterate over all documents in a run
    pub fn iter(&self, run_id: &RunId) -> Result<impl Iterator<Item = Result<JsonDoc>>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let prefix = Key::new_json_prefix(self.namespace_for_run(run_id));

        let entries = snapshot.scan_prefix(&prefix)?;
        Ok(entries.into_iter().map(|(_, vv)| {
            Self::deserialize_doc(&vv.value)
        }))
    }
}
```

### Acceptance Criteria

- [ ] exists() uses SnapshotView (fast path)
- [ ] list() returns all doc IDs for run
- [ ] list() uses prefix scan on unified storage
- [ ] count() returns correct count
- [ ] iter() allows streaming access

### Testing

```rust
#[test]
fn test_exists() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    assert!(!json.exists(&run_id, &doc_id).unwrap());

    json.create(&run_id, &doc_id, JsonValue::from(42)).unwrap();
    assert!(json.exists(&run_id, &doc_id).unwrap());
}

#[test]
fn test_list() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();

    let doc_ids: Vec<_> = (0..5).map(|_| JsonDocId::new()).collect();
    for doc_id in &doc_ids {
        json.create(&run_id, doc_id, JsonValue::from(42)).unwrap();
    }

    let listed = json.list(&run_id).unwrap();
    assert_eq!(listed.len(), 5);

    for doc_id in &doc_ids {
        assert!(listed.contains(doc_id));
    }
}

#[test]
fn test_list_filters_by_run() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);

    let run1 = RunId::new();
    let run2 = RunId::new();

    json.create(&run1, &JsonDocId::new(), JsonValue::from(1)).unwrap();
    json.create(&run1, &JsonDocId::new(), JsonValue::from(2)).unwrap();
    json.create(&run2, &JsonDocId::new(), JsonValue::from(3)).unwrap();

    assert_eq!(json.list(&run1).unwrap().len(), 2);
    assert_eq!(json.list(&run2).unwrap().len(), 1);
}

#[test]
fn test_count() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();

    assert_eq!(json.count(&run_id).unwrap(), 0);

    json.create(&run_id, &JsonDocId::new(), JsonValue::from(1)).unwrap();
    json.create(&run_id, &JsonDocId::new(), JsonValue::from(2)).unwrap();

    assert_eq!(json.count(&run_id).unwrap(), 2);
}
```

---

## Story #239: Serialization

**File**: `crates/primitives/src/json_store.rs`

**Deliverable**: MessagePack serialization for documents

### Implementation

```rust
impl JsonStore {
    /// Serialize document to Value for storage
    pub fn serialize_doc(doc: &JsonDoc) -> Result<Value> {
        let bytes = rmp_serde::to_vec(doc)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        Ok(Value::Bytes(bytes))
    }

    /// Deserialize document from Value
    pub fn deserialize_doc(value: &Value) -> Result<JsonDoc> {
        match value {
            Value::Bytes(bytes) => {
                rmp_serde::from_slice(bytes)
                    .map_err(|e| Error::Deserialization(e.to_string()))
            }
            _ => Err(Error::InvalidType("expected bytes for JsonDoc".into())),
        }
    }

    /// Get serialized size of document in bytes
    pub fn doc_size(doc: &JsonDoc) -> Result<usize> {
        let bytes = rmp_serde::to_vec(doc)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        Ok(bytes.len())
    }

    /// Validate document size limit
    pub fn validate_doc_size(doc: &JsonDoc) -> Result<()> {
        let size = Self::doc_size(doc)?;
        if size > MAX_DOCUMENT_SIZE {
            return Err(Error::Validation(
                JsonValidationError::DocumentTooLarge {
                    size: size as u64,
                    max: MAX_DOCUMENT_SIZE as u64,
                }
            ));
        }
        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] serialize_doc() produces MessagePack bytes
- [ ] deserialize_doc() reconstructs document
- [ ] Round-trip preserves all fields
- [ ] validate_doc_size() enforces 16MB limit

### Testing

```rust
#[test]
fn test_serialize_deserialize_roundtrip() {
    let doc = JsonDoc::new(
        JsonDocId::new(),
        JsonValue::from("test value"),
    );

    let serialized = JsonStore::serialize_doc(&doc).unwrap();
    let deserialized = JsonStore::deserialize_doc(&serialized).unwrap();

    assert_eq!(doc.id, deserialized.id);
    assert_eq!(doc.value, deserialized.value);
    assert_eq!(doc.version, deserialized.version);
}

#[test]
fn test_serialize_complex_document() {
    let mut obj = IndexMap::new();
    obj.insert("string".to_string(), JsonValue::from("hello"));
    obj.insert("number".to_string(), JsonValue::from(42));
    obj.insert("array".to_string(), JsonValue::Array(vec![
        JsonValue::from(1),
        JsonValue::from(2),
        JsonValue::from(3),
    ]));
    obj.insert("nested".to_string(), JsonValue::Object({
        let mut inner = IndexMap::new();
        inner.insert("foo".to_string(), JsonValue::from("bar"));
        inner
    }));

    let doc = JsonDoc::new(JsonDocId::new(), JsonValue::Object(obj));

    let serialized = JsonStore::serialize_doc(&doc).unwrap();
    let deserialized = JsonStore::deserialize_doc(&serialized).unwrap();

    assert_eq!(doc.value, deserialized.value);
}

#[test]
fn test_doc_size() {
    let small_doc = JsonDoc::new(JsonDocId::new(), JsonValue::from(42));
    let size = JsonStore::doc_size(&small_doc).unwrap();
    assert!(size < 1000); // Small document should be tiny

    // Validate passes for small doc
    JsonStore::validate_doc_size(&small_doc).unwrap();
}

#[test]
fn test_doc_size_limit() {
    // Create a document that exceeds the limit
    let large_string = "x".repeat(MAX_DOCUMENT_SIZE + 1);
    let large_doc = JsonDoc::new(
        JsonDocId::new(),
        JsonValue::from(large_string),
    );

    let result = JsonStore::validate_doc_size(&large_doc);
    assert!(result.is_err());
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/json_types.rs` | MODIFY - Add JsonDoc |
| `crates/primitives/src/json_store.rs` | CREATE - Stateless JsonStore facade |
| `crates/primitives/src/lib.rs` | MODIFY - Export json_store module |

---

## Success Criteria

- [ ] JsonStore holds ONLY `Arc<Database>` (stateless facade)
- [ ] JsonDoc stores value, version, created_at, updated_at
- [ ] Documents stored via unified Key::new_json() in ShardedStore
- [ ] Fast path reads use SnapshotView
- [ ] CRUD operations work correctly
- [ ] Version increments on every mutation
- [ ] Serialization roundtrips correctly
- [ ] Document size validated on storage
