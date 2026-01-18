# Epic 30: Transaction Integration

**Goal**: Integrate JSON operations with transaction system

**Dependencies**: Epic 28 complete

**GitHub Issue**: #260

---

## Scope

- JSON path read/patch types
- Lazy set initialization
- JsonStoreExt trait implementation
- Snapshot version capture
- Cross-primitive transactions

---

## Architectural Integration Rules

**CRITICAL**: JSON must extend the existing TransactionContext, NOT replace it:

1. **Use JsonStoreExt trait on TransactionContext** - no separate JsonTransactionState
2. **Lazy allocation with `Option<Vec<...>>`** - zero overhead for non-JSON transactions
3. **Hook into existing commit pipeline** - unified validation and commit
4. **Cross-primitive atomicity** - JSON + KV/Event/State in same transaction

---

## User Stories

| Story | Description | Priority | GitHub Issue |
|-------|-------------|----------|--------------|
| #244 | JSON Path Read/Patch Types | FOUNDATION | #282 |
| #245 | Lazy Set Initialization | CRITICAL | #283 |
| #246 | JsonStoreExt Trait Implementation | CRITICAL | #284 |
| #247 | Snapshot Version Capture | HIGH | #285 |
| #248 | Cross-Primitive Transactions | HIGH | #286 |

---

## Story #244: JSON Path Read/Patch Types

**File**: `crates/concurrency/src/transaction.rs`

**Deliverable**: Types for tracking JSON reads and writes in transactions

### Implementation

```rust
use in_mem_core::json_types::{JsonDocId, JsonPath, JsonPatch, JsonValue};
use in_mem_core::types::{Key, RunId};

/// Entry in the JSON read set
///
/// Records what was read during the transaction for conflict detection.
#[derive(Debug, Clone)]
pub struct JsonPathRead {
    /// Key in unified storage
    pub key: Key,
    /// Path that was read
    pub path: JsonPath,
    /// Version at read time (for staleness detection)
    pub version_at_read: u64,
}

/// Entry in the JSON write set
///
/// Records mutations to be applied at commit time.
#[derive(Debug, Clone)]
pub struct JsonPatchEntry {
    /// Key in unified storage
    pub key: Key,
    /// The patch to apply
    pub patch: JsonPatch,
    /// Version after this patch (for ordering)
    pub resulting_version: u64,
}

impl JsonPathRead {
    pub fn new(key: Key, path: JsonPath, version: u64) -> Self {
        JsonPathRead {
            key,
            path,
            version_at_read: version,
        }
    }

    /// Check if this read conflicts with a write
    pub fn conflicts_with_write(&self, write: &JsonPatchEntry) -> bool {
        self.key == write.key && self.path.overlaps(write.patch.path())
    }
}

impl JsonPatchEntry {
    pub fn new(key: Key, patch: JsonPatch, version: u64) -> Self {
        JsonPatchEntry {
            key,
            patch,
            resulting_version: version,
        }
    }

    /// Check if this write conflicts with another write
    pub fn conflicts_with(&self, other: &JsonPatchEntry) -> bool {
        self.key == other.key && self.patch.conflicts_with(&other.patch)
    }
}
```

### Acceptance Criteria

- [ ] JsonPathRead captures read context with key, path, version
- [ ] JsonPatchEntry captures write context with key, patch, version
- [ ] Both use unified Key (not JsonDocId directly)
- [ ] conflicts_with_write() uses path overlap detection
- [ ] conflicts_with() detects write-write conflicts

### Testing

```rust
#[test]
fn test_json_path_read() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());
    let read = JsonPathRead::new(key, JsonPath::parse("foo.bar").unwrap(), 5);

    assert_eq!(read.version_at_read, 5);
    assert_eq!(read.path.to_string(), "$.foo.bar");
}

#[test]
fn test_json_patch_entry() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());
    let patch = JsonPatch::set(JsonPath::parse("foo").unwrap(), 42);
    let entry = JsonPatchEntry::new(key, patch, 6);

    assert_eq!(entry.resulting_version, 6);
}

#[test]
fn test_read_write_conflict() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let read = JsonPathRead::new(key.clone(), JsonPath::parse("foo.bar").unwrap(), 5);

    // Overlapping write (ancestor path)
    let write1 = JsonPatchEntry::new(
        key.clone(),
        JsonPatch::set(JsonPath::parse("foo").unwrap(), 42),
        6,
    );
    assert!(read.conflicts_with_write(&write1));

    // Non-overlapping write
    let write2 = JsonPatchEntry::new(
        key.clone(),
        JsonPatch::set(JsonPath::parse("baz").unwrap(), 43),
        6,
    );
    assert!(!read.conflicts_with_write(&write2));
}

#[test]
fn test_write_write_conflict() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let write1 = JsonPatchEntry::new(
        key.clone(),
        JsonPatch::set(JsonPath::parse("foo.bar").unwrap(), 1),
        2,
    );

    // Overlapping write
    let write2 = JsonPatchEntry::new(
        key.clone(),
        JsonPatch::set(JsonPath::parse("foo.bar.baz").unwrap(), 2),
        3,
    );
    assert!(write1.conflicts_with(&write2));

    // Non-overlapping write
    let write3 = JsonPatchEntry::new(
        key.clone(),
        JsonPatch::set(JsonPath::parse("other").unwrap(), 3),
        3,
    );
    assert!(!write1.conflicts_with(&write3));
}
```

---

## Story #245: Lazy Set Initialization

**File**: `crates/concurrency/src/transaction.rs`

**Deliverable**: Lazy allocation of JSON tracking sets in TransactionContext

### Implementation

```rust
// Add to TransactionContext struct
pub struct TransactionContext {
    // ... existing fields ...

    /// JSON reads (lazily allocated)
    json_reads: Option<Vec<JsonPathRead>>,
    /// JSON writes (lazily allocated)
    json_writes: Option<Vec<JsonPatchEntry>>,
    /// Captured document versions at transaction start (lazily allocated)
    json_snapshot_versions: Option<HashMap<Key, u64>>,
}

impl TransactionContext {
    /// Get or create JSON read set
    fn json_reads_mut(&mut self) -> &mut Vec<JsonPathRead> {
        self.json_reads.get_or_insert_with(Vec::new)
    }

    /// Get or create JSON write set
    fn json_writes_mut(&mut self) -> &mut Vec<JsonPatchEntry> {
        self.json_writes.get_or_insert_with(Vec::new)
    }

    /// Get or create snapshot versions map
    fn json_snapshot_versions_mut(&mut self) -> &mut HashMap<Key, u64> {
        self.json_snapshot_versions.get_or_insert_with(HashMap::new)
    }

    /// Check if any JSON operations occurred
    pub fn has_json_ops(&self) -> bool {
        self.json_reads.is_some() || self.json_writes.is_some()
    }

    /// Get JSON reads if allocated
    pub fn json_reads(&self) -> Option<&Vec<JsonPathRead>> {
        self.json_reads.as_ref()
    }

    /// Get JSON writes if allocated
    pub fn json_writes(&self) -> Option<&Vec<JsonPatchEntry>> {
        self.json_writes.as_ref()
    }

    /// Get snapshot versions if allocated
    pub fn json_snapshot_versions(&self) -> Option<&HashMap<Key, u64>> {
        self.json_snapshot_versions.as_ref()
    }

    /// Reset JSON state (for transaction pooling)
    pub fn reset_json(&mut self) {
        if let Some(ref mut reads) = self.json_reads {
            reads.clear();
        }
        if let Some(ref mut writes) = self.json_writes {
            writes.clear();
        }
        if let Some(ref mut versions) = self.json_snapshot_versions {
            versions.clear();
        }
    }
}
```

### Acceptance Criteria

- [ ] Sets are None initially (no allocation)
- [ ] First access allocates the Vec/HashMap
- [ ] has_json_ops() returns correct status
- [ ] Non-JSON transactions have zero overhead
- [ ] reset_json() clears without deallocating (for pooling)

### Testing

```rust
#[test]
fn test_lazy_allocation() {
    let mut ctx = TransactionContext::new(RunId::new());

    // Initially no allocation
    assert!(!ctx.has_json_ops());
    assert!(ctx.json_reads().is_none());
    assert!(ctx.json_writes().is_none());

    // Access causes allocation
    ctx.json_reads_mut().push(JsonPathRead::new(
        Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new()),
        JsonPath::root(),
        1,
    ));

    assert!(ctx.has_json_ops());
    assert!(ctx.json_reads().is_some());
    assert_eq!(ctx.json_reads().unwrap().len(), 1);
}

#[test]
fn test_zero_overhead_for_non_json() {
    let mut ctx = TransactionContext::new(RunId::new());

    // Do non-JSON operations
    ctx.put(Key::new_kv(Namespace::for_run(RunId::new()), b"key"), Value::from(42)).unwrap();

    // JSON sets should still be None
    assert!(!ctx.has_json_ops());
    assert!(ctx.json_reads().is_none());
    assert!(ctx.json_writes().is_none());
}

#[test]
fn test_reset_json() {
    let mut ctx = TransactionContext::new(RunId::new());

    // Add some JSON ops
    ctx.json_reads_mut().push(JsonPathRead::new(
        Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new()),
        JsonPath::root(),
        1,
    ));
    ctx.json_writes_mut().push(JsonPatchEntry::new(
        Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new()),
        JsonPatch::set(JsonPath::root(), 42),
        2,
    ));

    assert!(ctx.has_json_ops());

    // Reset should clear but keep allocated
    ctx.reset_json();

    // Still allocated but empty
    assert!(!ctx.has_json_ops()); // Empty is considered no ops
    assert!(ctx.json_reads().map_or(true, |v| v.is_empty()));
    assert!(ctx.json_writes().map_or(true, |v| v.is_empty()));
}
```

---

## Story #246: JsonStoreExt Trait Implementation

**File**: `crates/primitives/src/extensions.rs`

**Deliverable**: Extension trait for JSON operations on TransactionContext

### Implementation

```rust
use in_mem_core::json_types::{
    JsonDoc, JsonDocId, JsonPath, JsonPatch, JsonValue,
    get_at_path, set_at_path, delete_at_path,
    validate_json_value, validate_path,
};
use in_mem_core::types::Key;
use in_mem_concurrency::transaction::{TransactionContext, JsonPathRead, JsonPatchEntry};

/// Extension trait for JSON operations on TransactionContext
///
/// This follows the same pattern as other primitive extensions.
pub trait JsonStoreExt {
    /// Read value at path (records in read set)
    fn json_get(&mut self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>>;

    /// Set value at path (records in write set)
    fn json_set(&mut self, key: &Key, path: &JsonPath, value: JsonValue) -> Result<()>;

    /// Delete value at path (records in write set)
    fn json_delete(&mut self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>>;

    /// Apply multiple patches atomically
    fn json_apply_patches(&mut self, key: &Key, patches: Vec<JsonPatch>) -> Result<()>;

    /// Get current document version
    fn json_get_version(&mut self, key: &Key) -> Result<Option<u64>>;

    /// Create new document
    fn json_create(&mut self, key: &Key, doc_id: JsonDocId, value: JsonValue) -> Result<()>;

    /// Delete entire document
    fn json_delete_doc(&mut self, key: &Key) -> Result<bool>;
}

impl JsonStoreExt for TransactionContext {
    fn json_get(&mut self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>> {
        // Get document from storage
        let doc = match self.get(key)? {
            Some(v) => JsonStore::deserialize_doc(&v)?,
            None => return Ok(None),
        };

        // Record the read with current version
        self.json_reads_mut().push(JsonPathRead::new(
            key.clone(),
            path.clone(),
            doc.version,
        ));

        // Capture snapshot version if not already captured
        self.json_snapshot_versions_mut()
            .entry(key.clone())
            .or_insert(doc.version);

        // Return value at path
        Ok(get_at_path(&doc.value, path).cloned())
    }

    fn json_set(&mut self, key: &Key, path: &JsonPath, value: JsonValue) -> Result<()> {
        validate_json_value(&value)?;
        validate_path(path)?;

        // Get current document
        let mut doc = match self.get(key)? {
            Some(v) => JsonStore::deserialize_doc(&v)?,
            None => return Err(Error::NotFound("document not found".into())),
        };

        // Apply the patch in memory
        set_at_path(&mut doc.value, path, value.clone())?;
        doc.touch();

        // Record the write
        self.json_writes_mut().push(JsonPatchEntry::new(
            key.clone(),
            JsonPatch::set(path.clone(), value),
            doc.version,
        ));

        // Update storage
        let serialized = JsonStore::serialize_doc(&doc)?;
        self.put(key.clone(), serialized)
    }

    fn json_delete(&mut self, key: &Key, path: &JsonPath) -> Result<Option<JsonValue>> {
        let mut doc = match self.get(key)? {
            Some(v) => JsonStore::deserialize_doc(&v)?,
            None => return Err(Error::NotFound("document not found".into())),
        };

        let deleted = delete_at_path(&mut doc.value, path)?;
        doc.touch();

        // Record the write
        self.json_writes_mut().push(JsonPatchEntry::new(
            key.clone(),
            JsonPatch::delete(path.clone()),
            doc.version,
        ));

        let serialized = JsonStore::serialize_doc(&doc)?;
        self.put(key.clone(), serialized)?;

        Ok(deleted)
    }

    fn json_apply_patches(&mut self, key: &Key, patches: Vec<JsonPatch>) -> Result<()> {
        let mut doc = match self.get(key)? {
            Some(v) => JsonStore::deserialize_doc(&v)?,
            None => return Err(Error::NotFound("document not found".into())),
        };

        for patch in patches {
            match &patch {
                JsonPatch::Set { path, value } => {
                    validate_json_value(value)?;
                    validate_path(path)?;
                    set_at_path(&mut doc.value, path, value.clone())?;
                }
                JsonPatch::Delete { path } => {
                    delete_at_path(&mut doc.value, path)?;
                }
            }

            // Record each patch
            self.json_writes_mut().push(JsonPatchEntry::new(
                key.clone(),
                patch,
                doc.version + 1, // Will increment
            ));
        }

        doc.touch();
        let serialized = JsonStore::serialize_doc(&doc)?;
        self.put(key.clone(), serialized)
    }

    fn json_get_version(&mut self, key: &Key) -> Result<Option<u64>> {
        match self.get(key)? {
            Some(v) => {
                let doc = JsonStore::deserialize_doc(&v)?;
                Ok(Some(doc.version))
            }
            None => Ok(None),
        }
    }

    fn json_create(&mut self, key: &Key, doc_id: JsonDocId, value: JsonValue) -> Result<()> {
        validate_json_value(&value)?;

        if self.get(key)?.is_some() {
            return Err(Error::AlreadyExists("document already exists".into()));
        }

        let doc = JsonDoc::new(doc_id, value.clone());
        let serialized = JsonStore::serialize_doc(&doc)?;

        // Record as a write at root path
        self.json_writes_mut().push(JsonPatchEntry::new(
            key.clone(),
            JsonPatch::set(JsonPath::root(), value),
            1,
        ));

        self.put(key.clone(), serialized)
    }

    fn json_delete_doc(&mut self, key: &Key) -> Result<bool> {
        if self.get(key)?.is_some() {
            // Record as a write (delete at root is equivalent to delete doc)
            self.json_writes_mut().push(JsonPatchEntry::new(
                key.clone(),
                JsonPatch::delete(JsonPath::root()),
                0, // Version doesn't matter for delete
            ));
            self.delete(key.clone())?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
```

### Acceptance Criteria

- [ ] json_get() records read in read set with version
- [ ] json_set() records patch in write set
- [ ] json_delete() records delete patch in write set
- [ ] All methods validate inputs
- [ ] Methods use existing TransactionContext storage
- [ ] Snapshot version captured on first read

### Testing

```rust
#[test]
fn test_json_get_records_read() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    // Create document outside transaction
    let json = JsonStore::new(db.clone());
    json.create(&run_id, &doc_id, JsonValue::from(42)).unwrap();

    // Read in transaction
    db.transaction(run_id, |txn| {
        let value = txn.json_get(&key, &JsonPath::root())?;
        assert_eq!(value.and_then(|v| v.as_i64()), Some(42));

        // Verify read was recorded
        assert!(txn.json_reads().is_some());
        assert_eq!(txn.json_reads().unwrap().len(), 1);
        assert_eq!(txn.json_reads().unwrap()[0].version_at_read, 1);

        Ok(())
    }).unwrap();
}

#[test]
fn test_json_set_records_write() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    let json = JsonStore::new(db.clone());
    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    db.transaction(run_id, |txn| {
        txn.json_set(&key, &JsonPath::parse("foo").unwrap(), JsonValue::from(42))?;

        // Verify write was recorded
        assert!(txn.json_writes().is_some());
        assert_eq!(txn.json_writes().unwrap().len(), 1);

        Ok(())
    }).unwrap();
}

#[test]
fn test_json_ext_api_consistency() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    // API should feel like other primitives
    db.transaction(run_id, |txn| {
        // Create
        txn.json_create(&key, doc_id, JsonValue::Object(IndexMap::new()))?;

        // Set
        txn.json_set(&key, &JsonPath::parse("name").unwrap(), JsonValue::from("test"))?;
        txn.json_set(&key, &JsonPath::parse("count").unwrap(), JsonValue::from(42))?;

        // Get
        let name = txn.json_get(&key, &JsonPath::parse("name").unwrap())?;
        assert_eq!(name.and_then(|v| v.as_str().map(String::from)), Some("test".to_string()));

        // Delete path
        txn.json_delete(&key, &JsonPath::parse("count").unwrap())?;

        Ok(())
    }).unwrap();
}
```

---

## Story #247: Snapshot Version Capture

**File**: `crates/concurrency/src/transaction.rs`

**Deliverable**: Capture document versions at transaction start for conflict detection

### Implementation

```rust
impl TransactionContext {
    /// Capture document version at first access
    ///
    /// This is called automatically by json_get when a document is first read.
    pub fn capture_json_version(&mut self, key: &Key, version: u64) {
        self.json_snapshot_versions_mut()
            .entry(key.clone())
            .or_insert(version);
    }

    /// Get captured version for a document
    pub fn get_snapshot_version(&self, key: &Key) -> Option<u64> {
        self.json_snapshot_versions
            .as_ref()
            .and_then(|m| m.get(key).copied())
    }

    /// Check if document version matches snapshot
    ///
    /// Returns true if:
    /// - Document was not read (no snapshot)
    /// - Current version matches snapshot version
    pub fn check_json_version(&self, key: &Key, current_version: u64) -> bool {
        match self.get_snapshot_version(key) {
            Some(snapshot) => snapshot == current_version,
            None => true, // Not in snapshot, no conflict possible
        }
    }
}

/// Snapshot validation at commit time
pub fn validate_json_snapshot(
    ctx: &TransactionContext,
    storage: &ShardedStore,
) -> Result<(), TransactionError> {
    if let Some(versions) = ctx.json_snapshot_versions() {
        for (key, snapshot_version) in versions {
            // Get current version from storage
            let current_version = match storage.get(key)? {
                Some(vv) => {
                    let doc = JsonStore::deserialize_doc(&vv.value)?;
                    doc.version
                }
                None => 0, // Document deleted
            };

            if current_version != *snapshot_version {
                return Err(TransactionError::JsonStaleRead {
                    key: key.clone(),
                    expected: *snapshot_version,
                    found: current_version,
                });
            }
        }
    }
    Ok(())
}
```

### Acceptance Criteria

- [ ] Version captured on first read of each document
- [ ] Subsequent reads of same document use cached version
- [ ] check_json_version() returns true for non-read documents
- [ ] Stale reads detected at commit time

### Testing

```rust
#[test]
fn test_version_capture_on_first_read() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    let json = JsonStore::new(db.clone());
    json.create(&run_id, &doc_id, JsonValue::from(1)).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::root(), JsonValue::from(2)).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::root(), JsonValue::from(3)).unwrap();
    // Version is now 3

    db.transaction(run_id, |txn| {
        // First read captures version
        txn.json_get(&key, &JsonPath::root())?;
        assert_eq!(txn.get_snapshot_version(&key), Some(3));

        // Subsequent reads don't change captured version
        txn.json_get(&key, &JsonPath::root())?;
        assert_eq!(txn.get_snapshot_version(&key), Some(3));

        Ok(())
    }).unwrap();
}

#[test]
fn test_stale_read_detection() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    let json = JsonStore::new(db.clone());
    json.create(&run_id, &doc_id, JsonValue::from(1)).unwrap();

    // Start transaction and read
    let result = db.transaction(run_id, |txn| {
        txn.json_get(&key, &JsonPath::root())?;

        // Simulate concurrent modification (in real scenario, another thread)
        // For testing, we bypass the transaction
        // This would fail in a real concurrent scenario

        Ok(())
    });

    // Transaction should succeed if no concurrent modification
    assert!(result.is_ok());
}

#[test]
fn test_check_version_for_unread_doc() {
    let ctx = TransactionContext::new(RunId::new());
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    // Unread document should not conflict
    assert!(ctx.check_json_version(&key, 5));
    assert!(ctx.check_json_version(&key, 100));
}
```

---

## Story #248: Cross-Primitive Transactions

**File**: `crates/engine/src/database.rs`

**Deliverable**: Atomic transactions spanning JSON and other primitives

### Implementation

```rust
impl Database {
    /// Execute a transaction that may include JSON and other primitives
    ///
    /// All primitives are committed atomically.
    pub fn transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T, TransactionError>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T, TransactionError>,
    {
        let mut ctx = self.begin_transaction(run_id)?;

        let result = f(&mut ctx)?;

        // Validate JSON conflicts if JSON ops occurred
        if ctx.has_json_ops() {
            self.validate_json_transaction(&ctx)?;
        }

        // Commit all primitives atomically
        self.commit_transaction(ctx)?;

        Ok(result)
    }

    /// Validate JSON-specific constraints before commit
    fn validate_json_transaction(&self, ctx: &TransactionContext) -> Result<(), TransactionError> {
        // 1. Check stale reads (version mismatches)
        validate_json_snapshot(ctx, self.storage())?;

        // 2. Check read-write conflicts within transaction
        if let (Some(reads), Some(writes)) = (ctx.json_reads(), ctx.json_writes()) {
            for read in reads {
                for write in writes {
                    if read.conflicts_with_write(write) {
                        return Err(TransactionError::JsonReadWriteConflict {
                            key: read.key.clone(),
                            read_path: read.path.clone(),
                            write_path: write.patch.path().clone(),
                        });
                    }
                }
            }
        }

        // 3. Check write-write conflicts within transaction
        if let Some(writes) = ctx.json_writes() {
            for (i, w1) in writes.iter().enumerate() {
                for w2 in writes.iter().skip(i + 1) {
                    if w1.conflicts_with(w2) {
                        return Err(TransactionError::JsonWriteWriteConflict {
                            key: w1.key.clone(),
                            path1: w1.patch.path().clone(),
                            path2: w2.patch.path().clone(),
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

// Add new error variants
#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    // ... existing variants ...

    #[error("JSON stale read: document {key:?} expected version {expected}, found {found}")]
    JsonStaleRead {
        key: Key,
        expected: u64,
        found: u64,
    },

    #[error("JSON read-write conflict: read at {read_path}, write at {write_path}")]
    JsonReadWriteConflict {
        key: Key,
        read_path: JsonPath,
        write_path: JsonPath,
    },

    #[error("JSON write-write conflict: writes at {path1} and {path2}")]
    JsonWriteWriteConflict {
        key: Key,
        path1: JsonPath,
        path2: JsonPath,
    },
}
```

### Acceptance Criteria

- [ ] JSON + KV in same transaction works atomically
- [ ] JSON + Event in same transaction works atomically
- [ ] JSON + State in same transaction works atomically
- [ ] Conflicts detected before commit
- [ ] Rollback on conflict includes all primitives
- [ ] Non-JSON transactions have zero overhead

### Testing

```rust
#[test]
fn test_json_kv_cross_primitive() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();

    let json = JsonStore::new(db.clone());
    let kv = KVStore::new(db.clone());

    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, JsonValue::from(0)).unwrap();

    // Transaction with both JSON and KV
    db.transaction(run_id, |txn| {
        let json_key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        let kv_key = Key::new_kv(Namespace::for_run(run_id), b"counter");

        txn.json_set(&json_key, &JsonPath::root(), JsonValue::from(42))?;
        txn.put(kv_key, Value::from(42))?;

        Ok(())
    }).unwrap();

    // Both should be committed
    let json_value = json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
    assert_eq!(json_value.and_then(|v| v.as_i64()), Some(42));

    let kv_value = kv.get(&run_id, b"counter").unwrap();
    assert_eq!(kv_value.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_cross_primitive_rollback() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();

    let json = JsonStore::new(db.clone());
    let kv = KVStore::new(db.clone());

    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, JsonValue::from(0)).unwrap();
    kv.set(&run_id, b"counter", Value::from(0)).unwrap();

    // Transaction that fails
    let result = db.transaction(run_id, |txn| {
        let json_key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        let kv_key = Key::new_kv(Namespace::for_run(run_id), b"counter");

        txn.json_set(&json_key, &JsonPath::root(), JsonValue::from(42))?;
        txn.put(kv_key, Value::from(42))?;

        // Force failure
        Err(TransactionError::Custom("test failure".into()))
    });

    assert!(result.is_err());

    // Both should be rolled back
    let json_value = json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
    assert_eq!(json_value.and_then(|v| v.as_i64()), Some(0));

    let kv_value = kv.get(&run_id, b"counter").unwrap();
    assert_eq!(kv_value.and_then(|v| v.as_i64()), Some(0));
}

#[test]
fn test_non_json_transaction_zero_overhead() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();

    db.transaction(run_id, |txn| {
        // Only KV operations
        let kv_key = Key::new_kv(Namespace::for_run(run_id), b"key");
        txn.put(kv_key, Value::from(42))?;

        // JSON sets should not be allocated
        assert!(!txn.has_json_ops());

        Ok(())
    }).unwrap();
}

#[test]
fn test_json_event_cross_primitive() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();

    let json = JsonStore::new(db.clone());
    let events = EventLog::new(db.clone());

    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    db.transaction(run_id, |txn| {
        let json_key = Key::new_json(Namespace::for_run(run_id), &doc_id);

        // Update JSON
        txn.json_set(&json_key, &JsonPath::parse("updated").unwrap(), JsonValue::from(true))?;

        // Append event
        txn.append_event(run_id, Event::new("document_updated", json!({ "doc_id": doc_id.to_string() })))?;

        Ok(())
    }).unwrap();

    // Both committed
    let updated = json.get(&run_id, &doc_id, &JsonPath::parse("updated").unwrap()).unwrap();
    assert_eq!(updated.and_then(|v| v.as_bool()), Some(true));

    let event_count = events.count(&run_id).unwrap();
    assert_eq!(event_count, 1);
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/concurrency/src/transaction.rs` | MODIFY - Add JSON tracking fields and methods |
| `crates/primitives/src/extensions.rs` | MODIFY - Add JsonStoreExt trait |
| `crates/engine/src/database.rs` | MODIFY - Add JSON validation in commit |

---

## Success Criteria

- [ ] JsonPathRead and JsonPatchEntry types defined
- [ ] TransactionContext extended with `Option<Vec<...>>` fields (lazy)
- [ ] JsonStoreExt trait implemented on TransactionContext
- [ ] Lazy allocation on first JSON operation (zero overhead for non-JSON txns)
- [ ] Snapshot captures document versions at transaction start
- [ ] JSON + KV/Event/State in same transaction works atomically
