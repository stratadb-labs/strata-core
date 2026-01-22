# Epic 29: WAL Integration

**Goal**: Integrate JSON operations with write-ahead logging

**Dependencies**: Epic 28 complete

**GitHub Issue**: #259

---

## Scope

- JSON WAL entry types (0x20-0x23)
- WAL write for JSON operations
- WAL replay for JSON
- Idempotent replay logic

---

## Architectural Integration Rules

**CRITICAL**: JSON WAL entries must integrate with the existing unified WAL:

1. **New entry variants (0x20-0x23) added to existing WALEntry enum** - no separate WAL
2. **WAL entries use unified Key** - not JsonDocId directly
3. **Patches recorded, never full documents** - efficient storage
4. **Replay uses version checks for idempotency** - safe recovery

---

## User Stories

| Story | Description | Priority | GitHub Issue |
|-------|-------------|----------|--------------|
| #240 | JSON WAL Entry Types (0x20-0x23) | CRITICAL | #278 |
| #241 | WAL Write for JSON Operations | CRITICAL | #279 |
| #242 | WAL Replay for JSON | CRITICAL | #280 |
| #243 | Idempotent Replay Logic | HIGH | #281 |

---

## Story #240: JSON WAL Entry Types

**File**: `crates/durability/src/wal.rs`

**Deliverable**: WAL entry types for JSON operations

### Implementation

```rust
/// WAL entry type constants for JSON operations
pub const WAL_JSON_CREATE: u8 = 0x20;
pub const WAL_JSON_SET: u8 = 0x21;
pub const WAL_JSON_DELETE: u8 = 0x22;
pub const WAL_JSON_DELETE_DOC: u8 = 0x23;

/// JSON WAL entry payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonWalEntry {
    /// Create new document
    Create {
        key: Key,  // Unified key, not JsonDocId directly
        value: JsonValue,
        version: u64,
    },
    /// Set value at path
    Set {
        key: Key,
        path: JsonPath,
        value: JsonValue,
        version: u64,
    },
    /// Delete value at path
    Delete {
        key: Key,
        path: JsonPath,
        version: u64,
    },
    /// Delete entire document
    DeleteDoc {
        key: Key,
    },
}

impl JsonWalEntry {
    pub fn entry_type(&self) -> u8 {
        match self {
            JsonWalEntry::Create { .. } => WAL_JSON_CREATE,
            JsonWalEntry::Set { .. } => WAL_JSON_SET,
            JsonWalEntry::Delete { .. } => WAL_JSON_DELETE,
            JsonWalEntry::DeleteDoc { .. } => WAL_JSON_DELETE_DOC,
        }
    }

    pub fn key(&self) -> &Key {
        match self {
            JsonWalEntry::Create { key, .. } => key,
            JsonWalEntry::Set { key, .. } => key,
            JsonWalEntry::Delete { key, .. } => key,
            JsonWalEntry::DeleteDoc { key, .. } => key,
        }
    }

    pub fn version(&self) -> Option<u64> {
        match self {
            JsonWalEntry::Create { version, .. } => Some(*version),
            JsonWalEntry::Set { version, .. } => Some(*version),
            JsonWalEntry::Delete { version, .. } => Some(*version),
            JsonWalEntry::DeleteDoc { .. } => None,
        }
    }
}

// Add to existing WALEntry enum:
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WALEntry {
    // ... existing variants ...
    Kv(KvWalEntry),
    Event(EventWalEntry),
    State(StateWalEntry),
    Trace(TraceWalEntry),

    // NEW: JSON variants
    Json(JsonWalEntry),
}

impl WALEntry {
    pub fn entry_type(&self) -> u8 {
        match self {
            // ... existing ...
            WALEntry::Json(json) => json.entry_type(),
        }
    }
}
```

### Acceptance Criteria

- [ ] All four entry types defined (Create, Set, Delete, DeleteDoc)
- [ ] Entry type constants match spec (0x20-0x23)
- [ ] Each entry uses unified Key (not JsonDocId directly)
- [ ] Entries contain version for idempotent replay
- [ ] Entries are serializable with MessagePack

### Testing

```rust
#[test]
fn test_json_wal_entry_types() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let create = JsonWalEntry::Create {
        key: key.clone(),
        value: JsonValue::from(42),
        version: 1,
    };
    assert_eq!(create.entry_type(), WAL_JSON_CREATE);

    let set = JsonWalEntry::Set {
        key: key.clone(),
        path: JsonPath::parse("foo").unwrap(),
        value: JsonValue::from(42),
        version: 2,
    };
    assert_eq!(set.entry_type(), WAL_JSON_SET);

    let delete = JsonWalEntry::Delete {
        key: key.clone(),
        path: JsonPath::parse("foo").unwrap(),
        version: 3,
    };
    assert_eq!(delete.entry_type(), WAL_JSON_DELETE);

    let delete_doc = JsonWalEntry::DeleteDoc { key: key.clone() };
    assert_eq!(delete_doc.entry_type(), WAL_JSON_DELETE_DOC);
}

#[test]
fn test_json_wal_entry_serialization() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());
    let entry = JsonWalEntry::Set {
        key,
        path: JsonPath::parse("foo.bar").unwrap(),
        value: JsonValue::from(42),
        version: 1,
    };

    let bytes = rmp_serde::to_vec(&entry).unwrap();
    let deserialized: JsonWalEntry = rmp_serde::from_slice(&bytes).unwrap();

    assert_eq!(entry.entry_type(), deserialized.entry_type());
    assert_eq!(entry.version(), deserialized.version());
}
```

---

## Story #241: WAL Write for JSON Operations

**File**: `crates/primitives/src/json_store.rs`

**Deliverable**: WAL writes for all JSON mutations

### Implementation

```rust
impl JsonStore {
    /// Internal: Write WAL entry and apply mutation
    ///
    /// All mutations go through this to ensure WAL-first semantics.
    fn write_wal_and_apply<F, T>(
        &self,
        run_id: &RunId,
        entry: JsonWalEntry,
        apply: F,
    ) -> Result<T>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T>,
    {
        self.db.transaction(*run_id, |txn| {
            // WAL is written as part of transaction commit
            txn.record_wal_entry(WALEntry::Json(entry));
            apply(txn)
        })
    }

    /// Create document with WAL
    pub fn create(&self, run_id: &RunId, doc_id: &JsonDocId, value: JsonValue) -> Result<()> {
        validate_json_value(&value)?;

        let key = self.key_for(run_id, doc_id);
        let entry = JsonWalEntry::Create {
            key: key.clone(),
            value: value.clone(),
            version: 1,
        };

        self.write_wal_and_apply(run_id, entry, |txn| {
            // Check doesn't already exist
            if txn.get(&key)?.is_some() {
                return Err(Error::AlreadyExists(format!("Document {}", doc_id)));
            }

            let doc = JsonDoc::new(*doc_id, value);
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key, serialized)
        })
    }

    /// Set at path with WAL
    pub fn set(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<u64> {
        validate_json_value(&value)?;
        validate_path(path)?;

        let key = self.key_for(run_id, doc_id);

        // Need to get current version before creating WAL entry
        let current_version = self.get_version(run_id, doc_id)?
            .ok_or_else(|| Error::NotFound(format!("Document {}", doc_id)))?;
        let new_version = current_version + 1;

        let entry = JsonWalEntry::Set {
            key: key.clone(),
            path: path.clone(),
            value: value.clone(),
            version: new_version,
        };

        self.write_wal_and_apply(run_id, entry, |txn| {
            let mut doc = match txn.get(&key)? {
                Some(v) => Self::deserialize_doc(&v)?,
                None => return Err(Error::NotFound(format!("Document {}", doc_id))),
            };

            set_at_path(&mut doc.value, path, value)?;
            doc.touch();

            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key, serialized)?;
            Ok(doc.version)
        })
    }

    /// Delete at path with WAL
    pub fn delete_at_path(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> {
        let key = self.key_for(run_id, doc_id);

        let current_version = self.get_version(run_id, doc_id)?
            .ok_or_else(|| Error::NotFound(format!("Document {}", doc_id)))?;
        let new_version = current_version + 1;

        let entry = JsonWalEntry::Delete {
            key: key.clone(),
            path: path.clone(),
            version: new_version,
        };

        self.write_wal_and_apply(run_id, entry, |txn| {
            let mut doc = match txn.get(&key)? {
                Some(v) => Self::deserialize_doc(&v)?,
                None => return Err(Error::NotFound(format!("Document {}", doc_id))),
            };

            let deleted = delete_at_path(&mut doc.value, path)?;
            doc.touch();

            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key, serialized)?;
            Ok(deleted)
        })
    }

    /// Delete document with WAL
    pub fn delete_doc(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool> {
        let key = self.key_for(run_id, doc_id);
        let entry = JsonWalEntry::DeleteDoc { key: key.clone() };

        self.write_wal_and_apply(run_id, entry, |txn| {
            if txn.get(&key)?.is_some() {
                txn.delete(key)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }
}
```

### Acceptance Criteria

- [ ] All mutations write WAL before state update
- [ ] WAL entries contain correct versions
- [ ] WAL entries use unified Key (not JsonDocId)
- [ ] Failures rollback correctly (transaction semantics)
- [ ] Patches never include full documents (only path + value)

### Testing

```rust
#[test]
fn test_create_writes_wal() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::from(42)).unwrap();

    // Verify WAL entry was written
    let wal_entries = db.wal().read_entries().unwrap();
    assert!(!wal_entries.is_empty());

    let last_entry = wal_entries.last().unwrap();
    match last_entry {
        WALEntry::Json(JsonWalEntry::Create { version, .. }) => {
            assert_eq!(*version, 1);
        }
        _ => panic!("Expected JSON Create WAL entry"),
    }
}

#[test]
fn test_set_writes_wal_with_version() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::parse("foo").unwrap(), JsonValue::from(42)).unwrap();

    let wal_entries = db.wal().read_entries().unwrap();
    let set_entry = wal_entries.iter().find(|e| matches!(e, WALEntry::Json(JsonWalEntry::Set { .. })));

    match set_entry {
        Some(WALEntry::Json(JsonWalEntry::Set { version, path, .. })) => {
            assert_eq!(*version, 2);
            assert_eq!(path.to_string(), "$.foo");
        }
        _ => panic!("Expected JSON Set WAL entry"),
    }
}

#[test]
fn test_wal_contains_patch_not_full_doc() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create large document
    let mut large_obj = IndexMap::new();
    for i in 0..1000 {
        large_obj.insert(format!("key{}", i), JsonValue::from(i as i64));
    }
    json.create(&run_id, &doc_id, JsonValue::Object(large_obj)).unwrap();

    // Set single value
    json.set(&run_id, &doc_id, &JsonPath::parse("new_key").unwrap(), JsonValue::from(42)).unwrap();

    // WAL entry should only contain the path and value, not the full document
    let wal_entries = db.wal().read_entries().unwrap();
    let set_entry = wal_entries.iter().find(|e| matches!(e, WALEntry::Json(JsonWalEntry::Set { .. })));

    match set_entry {
        Some(WALEntry::Json(JsonWalEntry::Set { value, .. })) => {
            // Value should just be 42, not the entire document
            assert_eq!(value.as_i64(), Some(42));
        }
        _ => panic!("Expected JSON Set WAL entry"),
    }
}
```

---

## Story #242: WAL Replay for JSON

**File**: `crates/durability/src/recovery.rs`

**Deliverable**: WAL replay to reconstruct JSON state

### Implementation

```rust
impl Recovery {
    /// Replay JSON WAL entry
    pub fn replay_json_entry(
        &self,
        storage: &ShardedStore,
        entry: &JsonWalEntry,
    ) -> Result<()> {
        match entry {
            JsonWalEntry::Create { key, value, version } => {
                self.replay_json_create(storage, key, value.clone(), *version)
            }
            JsonWalEntry::Set { key, path, value, version } => {
                self.replay_json_set(storage, key, path, value.clone(), *version)
            }
            JsonWalEntry::Delete { key, path, version } => {
                self.replay_json_delete(storage, key, path, *version)
            }
            JsonWalEntry::DeleteDoc { key } => {
                self.replay_json_delete_doc(storage, key)
            }
        }
    }

    fn replay_json_create(
        &self,
        storage: &ShardedStore,
        key: &Key,
        value: JsonValue,
        version: u64,
    ) -> Result<()> {
        // Extract doc_id from key
        let doc_id = JsonDocId::try_from_bytes(key.user_key())
            .ok_or_else(|| Error::InvalidKey("cannot parse JsonDocId from key".into()))?;

        let mut doc = JsonDoc::new(doc_id, value);
        doc.version = version;

        let serialized = JsonStore::serialize_doc(&doc)?;
        storage.put(key.clone(), serialized)?;
        Ok(())
    }

    fn replay_json_set(
        &self,
        storage: &ShardedStore,
        key: &Key,
        path: &JsonPath,
        value: JsonValue,
        version: u64,
    ) -> Result<()> {
        // Get current document
        let current = storage.get(key)?
            .ok_or_else(|| Error::NotFound("document not found during replay".into()))?;

        let mut doc = JsonStore::deserialize_doc(&current.value)?;

        // Skip if already applied (idempotent)
        if doc.version >= version {
            return Ok(());
        }

        // Apply patch
        set_at_path(&mut doc.value, path, value)?;
        doc.version = version;
        doc.updated_at = SystemTime::now();

        let serialized = JsonStore::serialize_doc(&doc)?;
        storage.put(key.clone(), serialized)?;
        Ok(())
    }

    fn replay_json_delete(
        &self,
        storage: &ShardedStore,
        key: &Key,
        path: &JsonPath,
        version: u64,
    ) -> Result<()> {
        let current = storage.get(key)?
            .ok_or_else(|| Error::NotFound("document not found during replay".into()))?;

        let mut doc = JsonStore::deserialize_doc(&current.value)?;

        // Skip if already applied
        if doc.version >= version {
            return Ok(());
        }

        delete_at_path(&mut doc.value, path)?;
        doc.version = version;
        doc.updated_at = SystemTime::now();

        let serialized = JsonStore::serialize_doc(&doc)?;
        storage.put(key.clone(), serialized)?;
        Ok(())
    }

    fn replay_json_delete_doc(
        &self,
        storage: &ShardedStore,
        key: &Key,
    ) -> Result<()> {
        storage.delete(key)?;
        Ok(())
    }
}

// Add to main replay loop
impl Recovery {
    pub fn replay_wal(&self, storage: &ShardedStore, wal: &WAL) -> Result<()> {
        for entry in wal.read_entries()? {
            match entry {
                // ... existing entries ...
                WALEntry::Json(json_entry) => {
                    self.replay_json_entry(storage, &json_entry)?;
                }
            }
        }
        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] replay_json_entry() handles all entry types
- [ ] Create replay reconstructs full document
- [ ] Set/Delete replay applies patches correctly
- [ ] State matches after replay
- [ ] Order is preserved

### Testing

```rust
#[test]
fn test_wal_replay_reconstructs_state() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create and modify document
    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::parse("foo").unwrap(), JsonValue::from(1)).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::parse("bar").unwrap(), JsonValue::from(2)).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::parse("foo").unwrap(), JsonValue::from(3)).unwrap(); // Overwrite

    let expected = json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();

    // Simulate crash and recovery
    let db2 = Arc::new(Database::recover(db.path()).unwrap());
    let json2 = JsonStore::new(db2);

    let recovered = json2.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
    assert_eq!(expected, recovered);
}

#[test]
fn test_wal_replay_preserves_order() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Operations in specific order
    json.create(&run_id, &doc_id, JsonValue::Array(vec![])).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::parse("[0]").unwrap(), JsonValue::from("first")).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::parse("[1]").unwrap(), JsonValue::from("second")).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::parse("[2]").unwrap(), JsonValue::from("third")).unwrap();

    // Recover
    let db2 = Arc::new(Database::recover(db.path()).unwrap());
    let json2 = JsonStore::new(db2);

    let arr = json2.get(&run_id, &doc_id, &JsonPath::root()).unwrap().unwrap();
    let arr = arr.as_array().unwrap();

    assert_eq!(arr[0].as_str(), Some("first"));
    assert_eq!(arr[1].as_str(), Some("second"));
    assert_eq!(arr[2].as_str(), Some("third"));
}

#[test]
fn test_wal_replay_handles_delete() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let mut obj = IndexMap::new();
    obj.insert("keep".to_string(), JsonValue::from(1));
    obj.insert("delete".to_string(), JsonValue::from(2));

    json.create(&run_id, &doc_id, JsonValue::Object(obj)).unwrap();
    json.delete_at_path(&run_id, &doc_id, &JsonPath::parse("delete").unwrap()).unwrap();

    // Recover
    let db2 = Arc::new(Database::recover(db.path()).unwrap());
    let json2 = JsonStore::new(db2);

    assert!(json2.get(&run_id, &doc_id, &JsonPath::parse("keep").unwrap()).unwrap().is_some());
    assert!(json2.get(&run_id, &doc_id, &JsonPath::parse("delete").unwrap()).unwrap().is_none());
}
```

---

## Story #243: Idempotent Replay Logic

**File**: `crates/durability/src/recovery.rs`

**Deliverable**: Version-based idempotent replay

### Implementation

```rust
impl Recovery {
    /// Check if JSON entry has already been applied
    pub fn is_json_entry_applied(
        &self,
        storage: &ShardedStore,
        entry: &JsonWalEntry,
    ) -> Result<bool> {
        match entry {
            JsonWalEntry::Create { key, .. } => {
                // Create is applied if document exists
                Ok(storage.get(key)?.is_some())
            }
            JsonWalEntry::Set { key, version, .. } |
            JsonWalEntry::Delete { key, version, .. } => {
                // Set/Delete applied if current version >= entry version
                match storage.get(key)? {
                    Some(vv) => {
                        let doc = JsonStore::deserialize_doc(&vv.value)?;
                        Ok(doc.version >= *version)
                    }
                    None => Ok(false),
                }
            }
            JsonWalEntry::DeleteDoc { key } => {
                // DeleteDoc applied if document doesn't exist
                Ok(storage.get(key)?.is_none())
            }
        }
    }

    /// Replay entry only if not already applied (idempotent)
    pub fn replay_json_entry_idempotent(
        &self,
        storage: &ShardedStore,
        entry: &JsonWalEntry,
    ) -> Result<bool> {
        if self.is_json_entry_applied(storage, entry)? {
            return Ok(false); // Already applied
        }
        self.replay_json_entry(storage, entry)?;
        Ok(true) // Applied
    }

    /// Replay all JSON WAL entries idempotently
    pub fn replay_json_wal_idempotent(
        &self,
        storage: &ShardedStore,
        entries: &[JsonWalEntry],
    ) -> Result<ReplayStats> {
        let mut stats = ReplayStats::default();

        for entry in entries {
            if self.replay_json_entry_idempotent(storage, entry)? {
                stats.applied += 1;
            } else {
                stats.skipped += 1;
            }
        }

        Ok(stats)
    }
}

#[derive(Debug, Default)]
pub struct ReplayStats {
    pub applied: usize,
    pub skipped: usize,
}
```

### Acceptance Criteria

- [ ] is_json_entry_applied() uses version comparison
- [ ] Duplicate replays are no-ops
- [ ] Returns whether entry was applied
- [ ] Stats track applied vs skipped entries

### Testing

```rust
#[test]
fn test_idempotent_replay_skips_duplicates() {
    let db = Arc::new(Database::open_temp().unwrap());
    let storage = db.storage();
    let recovery = Recovery::new();

    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let create_entry = JsonWalEntry::Create {
        key: key.clone(),
        value: JsonValue::from(42),
        version: 1,
    };

    // First replay should apply
    let applied = recovery.replay_json_entry_idempotent(storage, &create_entry).unwrap();
    assert!(applied);

    // Second replay should skip (already exists)
    let applied = recovery.replay_json_entry_idempotent(storage, &create_entry).unwrap();
    assert!(!applied);
}

#[test]
fn test_idempotent_replay_version_check() {
    let db = Arc::new(Database::open_temp().unwrap());
    let storage = db.storage();
    let recovery = Recovery::new();

    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    // Create document
    let create_entry = JsonWalEntry::Create {
        key: key.clone(),
        value: JsonValue::Object(IndexMap::new()),
        version: 1,
    };
    recovery.replay_json_entry(storage, &create_entry).unwrap();

    // Apply version 2
    let set_v2 = JsonWalEntry::Set {
        key: key.clone(),
        path: JsonPath::parse("foo").unwrap(),
        value: JsonValue::from(2),
        version: 2,
    };
    let applied = recovery.replay_json_entry_idempotent(storage, &set_v2).unwrap();
    assert!(applied);

    // Try to replay version 2 again - should skip
    let applied = recovery.replay_json_entry_idempotent(storage, &set_v2).unwrap();
    assert!(!applied);

    // Try to replay version 1 - should skip (older)
    let set_v1 = JsonWalEntry::Set {
        key: key.clone(),
        path: JsonPath::parse("bar").unwrap(),
        value: JsonValue::from(1),
        version: 1,
    };
    let applied = recovery.replay_json_entry_idempotent(storage, &set_v1).unwrap();
    assert!(!applied);

    // Apply version 3 - should succeed
    let set_v3 = JsonWalEntry::Set {
        key: key.clone(),
        path: JsonPath::parse("baz").unwrap(),
        value: JsonValue::from(3),
        version: 3,
    };
    let applied = recovery.replay_json_entry_idempotent(storage, &set_v3).unwrap();
    assert!(applied);
}

#[test]
fn test_replay_stats() {
    let db = Arc::new(Database::open_temp().unwrap());
    let storage = db.storage();
    let recovery = Recovery::new();

    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let entries = vec![
        JsonWalEntry::Create {
            key: key.clone(),
            value: JsonValue::from(1),
            version: 1,
        },
        JsonWalEntry::Set {
            key: key.clone(),
            path: JsonPath::parse("a").unwrap(),
            value: JsonValue::from(2),
            version: 2,
        },
        JsonWalEntry::Set {
            key: key.clone(),
            path: JsonPath::parse("b").unwrap(),
            value: JsonValue::from(3),
            version: 3,
        },
    ];

    // First replay - all applied
    let stats = recovery.replay_json_wal_idempotent(storage, &entries).unwrap();
    assert_eq!(stats.applied, 3);
    assert_eq!(stats.skipped, 0);

    // Second replay - all skipped
    let stats = recovery.replay_json_wal_idempotent(storage, &entries).unwrap();
    assert_eq!(stats.applied, 0);
    assert_eq!(stats.skipped, 3);
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/durability/src/wal.rs` | MODIFY - Add JSON WAL entry types |
| `crates/durability/src/recovery.rs` | MODIFY - Add JSON replay logic |
| `crates/primitives/src/json_store.rs` | MODIFY - Add WAL writes to mutations |

---

## Success Criteria

- [ ] WAL entry types 0x20-0x23 defined and serializable
- [ ] WAL entries use unified Key (not JsonDocId directly)
- [ ] All JSON mutations write WAL entries before storage
- [ ] WAL replay reconstructs document state correctly
- [ ] Replay is idempotent (version check skips already-applied entries)
- [ ] Patches never include full documents
