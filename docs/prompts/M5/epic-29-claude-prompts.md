# Epic 29: WAL Integration - Implementation Prompts

**Epic Goal**: Integrate JSON operations with Write-Ahead Log

**GitHub Issue**: [#259](https://github.com/anibjoshi/in-mem/issues/259)
**Status**: Ready after Epic 26
**Dependencies**: Epic 26 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M5_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M5_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M5/EPIC_29_WAL_INTEGRATION.md`
3. **Prompt Header**: `docs/prompts/M5/M5_PROMPT_HEADER.md` for the 6 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## CRITICAL: UNIFIED WAL (Rule 5)

**WAL entry types (0x20-0x23) are added to existing WALEntry enum. NO separate JSON WAL.**

```rust
// CORRECT: Extend existing enum
pub enum WALEntry {
    // Existing entries...
    Put { key: Key, value: Value },
    Delete { key: Key },

    // NEW: JSON entries
    JsonCreate { key: Key, doc: JsonDoc },           // 0x20
    JsonSet { key: Key, path: JsonPath, value: JsonValue, version: u64 }, // 0x21
    JsonDelete { key: Key, path: JsonPath, version: u64 },  // 0x22
    JsonDestroy { key: Key },                        // 0x23
}

// WRONG: Separate WAL
struct JsonWAL { ... }  // NEVER DO THIS
```

---

## Epic 29 Overview

### Scope
- JSON WAL entry types (0x20-0x23)
- WAL serialization for JSON operations
- Recovery/replay logic
- Integration with existing WAL infrastructure

### Success Criteria
- [ ] WALEntry enum extended with JSON variants
- [ ] JSON entries serialize/deserialize correctly
- [ ] Recovery replays JSON operations in order
- [ ] Crash recovery reconstructs JSON state correctly

### Component Breakdown
- **Story #240 (GitHub #278)**: JSON WAL Entry Types
- **Story #241 (GitHub #279)**: WAL Serialization
- **Story #242 (GitHub #280)**: Recovery/Replay Logic
- **Story #243 (GitHub #281)**: Crash Recovery Tests

---

## Dependency Graph

```
Story #278 (Entry Types) ──> Story #279 (Serialization) ──> Story #280 (Replay) ──> Story #281 (Tests)
```

---

## Story #278: JSON WAL Entry Types

**GitHub Issue**: [#278](https://github.com/anibjoshi/in-mem/issues/278)
**Estimated Time**: 2 hours
**Dependencies**: Epic 26 complete

### Start Story

```bash
gh issue view 278
./scripts/start-story.sh 29 278 json-wal-entries
```

### Implementation

Update `crates/durability/src/wal.rs`:

```rust
/// WAL entry type tags
pub mod entry_tags {
    pub const PUT: u8 = 0x01;
    pub const DELETE: u8 = 0x02;
    // ... existing tags ...

    // JSON entry tags (0x20-0x23)
    pub const JSON_CREATE: u8 = 0x20;
    pub const JSON_SET: u8 = 0x21;
    pub const JSON_DELETE: u8 = 0x22;
    pub const JSON_DESTROY: u8 = 0x23;
}

/// Write-ahead log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WALEntry {
    // Existing variants...
    Put { key: Key, value: Value },
    Delete { key: Key },

    // JSON operations
    JsonCreate {
        key: Key,
        doc: JsonDoc,
    },
    JsonSet {
        key: Key,
        path: JsonPath,
        value: JsonValue,
        version: u64,
    },
    JsonDelete {
        key: Key,
        path: JsonPath,
        version: u64,
    },
    JsonDestroy {
        key: Key,
    },
}

impl WALEntry {
    pub fn entry_tag(&self) -> u8 {
        match self {
            WALEntry::Put { .. } => entry_tags::PUT,
            WALEntry::Delete { .. } => entry_tags::DELETE,
            // ... existing ...
            WALEntry::JsonCreate { .. } => entry_tags::JSON_CREATE,
            WALEntry::JsonSet { .. } => entry_tags::JSON_SET,
            WALEntry::JsonDelete { .. } => entry_tags::JSON_DELETE,
            WALEntry::JsonDestroy { .. } => entry_tags::JSON_DESTROY,
        }
    }
}
```

### Tests

```rust
#[test]
fn test_json_entry_tags() {
    assert_eq!(entry_tags::JSON_CREATE, 0x20);
    assert_eq!(entry_tags::JSON_SET, 0x21);
    assert_eq!(entry_tags::JSON_DELETE, 0x22);
    assert_eq!(entry_tags::JSON_DESTROY, 0x23);
}

#[test]
fn test_json_entry_tag_method() {
    let entry = WALEntry::JsonCreate {
        key: Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new()),
        doc: JsonDoc::new(JsonDocId::new(), JsonValue::from(42)),
    };
    assert_eq!(entry.entry_tag(), entry_tags::JSON_CREATE);
}
```

### Complete Story

```bash
./scripts/complete-story.sh 278
```

---

## Story #279: WAL Serialization

**GitHub Issue**: [#279](https://github.com/anibjoshi/in-mem/issues/279)
**Estimated Time**: 3 hours
**Dependencies**: Story #278

### Start Story

```bash
gh issue view 279
./scripts/start-story.sh 29 279 wal-serialization
```

### Implementation

```rust
impl WALEntry {
    /// Serialize entry to bytes
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        buf.push(self.entry_tag());

        match self {
            WALEntry::JsonCreate { key, doc } => {
                let key_bytes = key.serialize()?;
                buf.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(&key_bytes);

                let doc_bytes = rmp_serde::to_vec(doc)?;
                buf.extend_from_slice(&(doc_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(&doc_bytes);
            }
            WALEntry::JsonSet { key, path, value, version } => {
                // Key
                let key_bytes = key.serialize()?;
                buf.extend_from_slice(&(key_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(&key_bytes);

                // Path
                let path_bytes = rmp_serde::to_vec(path)?;
                buf.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(&path_bytes);

                // Value
                let value_bytes = rmp_serde::to_vec(value)?;
                buf.extend_from_slice(&(value_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(&value_bytes);

                // Version
                buf.extend_from_slice(&version.to_le_bytes());
            }
            // ... other JSON variants ...
            _ => { /* existing serialization */ }
        }

        Ok(buf)
    }

    /// Deserialize entry from bytes
    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            return Err(Error::Deserialization("empty entry".into()));
        }

        let tag = bytes[0];
        let data = &bytes[1..];

        match tag {
            entry_tags::JSON_CREATE => {
                // Deserialize key and doc
                // ...
            }
            entry_tags::JSON_SET => {
                // Deserialize key, path, value, version
                // ...
            }
            // ... other JSON variants ...
            _ => { /* existing deserialization */ }
        }
    }
}
```

### Tests

```rust
#[test]
fn test_json_create_roundtrip() {
    let entry = WALEntry::JsonCreate {
        key: Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new()),
        doc: JsonDoc::new(JsonDocId::new(), JsonValue::from(42)),
    };

    let serialized = entry.serialize().unwrap();
    let deserialized = WALEntry::deserialize(&serialized).unwrap();

    match (entry, deserialized) {
        (WALEntry::JsonCreate { doc: d1, .. }, WALEntry::JsonCreate { doc: d2, .. }) => {
            assert_eq!(d1.value, d2.value);
        }
        _ => panic!("type mismatch"),
    }
}

#[test]
fn test_json_set_roundtrip() {
    let entry = WALEntry::JsonSet {
        key: Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new()),
        path: JsonPath::parse("foo.bar").unwrap(),
        value: JsonValue::from(42),
        version: 5,
    };

    let serialized = entry.serialize().unwrap();
    let deserialized = WALEntry::deserialize(&serialized).unwrap();

    match deserialized {
        WALEntry::JsonSet { path, value, version, .. } => {
            assert_eq!(path.to_string(), "$.foo.bar");
            assert_eq!(value.as_i64(), Some(42));
            assert_eq!(version, 5);
        }
        _ => panic!("type mismatch"),
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 279
```

---

## Story #280: Recovery/Replay Logic

**GitHub Issue**: [#280](https://github.com/anibjoshi/in-mem/issues/280)
**Estimated Time**: 3 hours
**Dependencies**: Story #279

### Start Story

```bash
gh issue view 280
./scripts/start-story.sh 29 280 wal-replay
```

### Implementation

Update `crates/engine/src/recovery.rs`:

```rust
impl Recovery {
    /// Replay WAL entries to storage
    pub fn replay_entry(
        &mut self,
        storage: &mut Storage,
        entry: &WALEntry,
    ) -> Result<()> {
        match entry {
            // Existing entry handling...

            WALEntry::JsonCreate { key, doc } => {
                let serialized = JsonStore::serialize_doc(doc)?;
                storage.put(key.clone(), Value::Bytes(serialized))?;
            }

            WALEntry::JsonSet { key, path, value, version } => {
                // Load current doc
                let current = storage.get(key)?
                    .ok_or(Error::NotFound(format!("JSON doc for WAL replay")))?;
                let mut doc = JsonStore::deserialize_doc(current.value.as_bytes()?)?;

                // Apply mutation
                set_at_path(&mut doc.value, path, value.clone())?;
                doc.version = *version;
                doc.updated_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;

                // Store updated doc
                let serialized = JsonStore::serialize_doc(&doc)?;
                storage.put(key.clone(), Value::Bytes(serialized))?;
            }

            WALEntry::JsonDelete { key, path, version } => {
                let current = storage.get(key)?
                    .ok_or(Error::NotFound(format!("JSON doc for WAL replay")))?;
                let mut doc = JsonStore::deserialize_doc(current.value.as_bytes()?)?;

                delete_at_path(&mut doc.value, path)?;
                doc.version = *version;

                let serialized = JsonStore::serialize_doc(&doc)?;
                storage.put(key.clone(), Value::Bytes(serialized))?;
            }

            WALEntry::JsonDestroy { key } => {
                storage.delete(key)?;
            }
        }

        Ok(())
    }
}
```

### Tests

```rust
#[test]
fn test_json_replay_create_then_set() {
    let mut storage = Storage::new_temp().unwrap();
    let mut recovery = Recovery::new();

    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    // Replay create
    let create_entry = WALEntry::JsonCreate {
        key: key.clone(),
        doc: JsonDoc::new(JsonDocId::new(), JsonValue::from(1)),
    };
    recovery.replay_entry(&mut storage, &create_entry).unwrap();

    // Replay set
    let set_entry = WALEntry::JsonSet {
        key: key.clone(),
        path: JsonPath::root(),
        value: JsonValue::from(42),
        version: 2,
    };
    recovery.replay_entry(&mut storage, &set_entry).unwrap();

    // Verify final state
    let stored = storage.get(&key).unwrap().unwrap();
    let doc = JsonStore::deserialize_doc(stored.value.as_bytes().unwrap()).unwrap();
    assert_eq!(doc.value.as_i64(), Some(42));
    assert_eq!(doc.version, 2);
}
```

### Complete Story

```bash
./scripts/complete-story.sh 280
```

---

## Story #281: Crash Recovery Tests

**GitHub Issue**: [#281](https://github.com/anibjoshi/in-mem/issues/281)
**Estimated Time**: 3 hours
**Dependencies**: Story #280

### Start Story

```bash
gh issue view 281
./scripts/start-story.sh 29 281 crash-recovery-tests
```

### Implementation

Create `crates/primitives/tests/json_recovery.rs`:

```rust
#[test]
fn test_json_crash_recovery() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().to_path_buf();

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create and modify, then close (simulating crash before explicit flush)
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let json = JsonStore::new(db);

        json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();
        json.set(&run_id, &doc_id, &JsonPath::parse("version").unwrap(), JsonValue::from(1)).unwrap();
        json.set(&run_id, &doc_id, &JsonPath::parse("version").unwrap(), JsonValue::from(2)).unwrap();
        json.set(&run_id, &doc_id, &JsonPath::parse("version").unwrap(), JsonValue::from(3)).unwrap();
        // Drop without explicit close - simulates crash
    }

    // Recover and verify
    {
        let db = Arc::new(Database::recover(&path).unwrap());
        let json = JsonStore::new(db);

        let version = json.get(&run_id, &doc_id, &JsonPath::parse("version").unwrap()).unwrap();
        assert_eq!(version.and_then(|v| v.as_i64()), Some(3));
    }
}

#[test]
fn test_json_recovery_with_delete() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().to_path_buf();

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    {
        let db = Arc::new(Database::open(&path).unwrap());
        let json = JsonStore::new(db);

        let mut obj = IndexMap::new();
        obj.insert("a".to_string(), JsonValue::from(1));
        obj.insert("b".to_string(), JsonValue::from(2));
        json.create(&run_id, &doc_id, JsonValue::Object(obj)).unwrap();

        json.delete_at_path(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap();
    }

    {
        let db = Arc::new(Database::recover(&path).unwrap());
        let json = JsonStore::new(db);

        assert!(json.get(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap().is_none());
        assert!(json.get(&run_id, &doc_id, &JsonPath::parse("b").unwrap()).unwrap().is_some());
    }
}

#[test]
fn test_json_recovery_destroy() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().to_path_buf();

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    {
        let db = Arc::new(Database::open(&path).unwrap());
        let json = JsonStore::new(db);

        json.create(&run_id, &doc_id, JsonValue::from(42)).unwrap();
        json.destroy(&run_id, &doc_id).unwrap();
    }

    {
        let db = Arc::new(Database::recover(&path).unwrap());
        let json = JsonStore::new(db);

        assert!(!json.exists(&run_id, &doc_id).unwrap());
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 281
```

---

## Epic 29 Completion Checklist

### Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-durability -- json
~/.cargo/bin/cargo test -p in-mem-primitives -- json_recovery
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
```

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-29-wal-integration -m "Epic 29: WAL Integration complete

Delivered:
- JSON WAL entry types (0x20-0x23)
- WAL serialization for JSON operations
- Recovery/replay logic
- Crash recovery tests

Stories: #278, #279, #280, #281
"
git push origin develop
gh issue close 259 --comment "Epic 29: WAL Integration - COMPLETE"
```
