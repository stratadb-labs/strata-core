# M5 Integration Analysis: JSON Primitive Alignment with Existing Architecture

## Executive Summary

This document analyzes the M5 JSON primitive design against the existing M2/M3/M4 architecture to identify:
1. **Potential duplication of functionality**
2. **API inconsistencies with other primitives**
3. **Integration gaps with existing infrastructure**
4. **Recommendations for proper alignment**

### Key Findings

| Area | Finding | Severity | Recommendation |
|------|---------|----------|----------------|
| **Storage Layer** | M5 proposes DashMap-based JsonStore | 🔴 CRITICAL | Use unified storage (ShardedStore) with TypeTag |
| **Value Types** | M5 defines new JsonValue enum | 🟡 MEDIUM | Evaluate reuse of existing `Value` enum |
| **Transaction Integration** | M5 proposes separate read/write sets | 🟡 MEDIUM | Extend existing TransactionContext pattern |
| **WAL Integration** | M5 proposes new WAL entry types | 🟢 ALIGNED | Fits existing pattern (0x20-0x23 reserved) |
| **API Pattern** | M5 has doc-centric vs key-centric API | 🟡 MEDIUM | Align with stateless facade pattern |

---

## 1. Storage Layer Analysis

### Existing Architecture (M3/M4)

The existing primitives use a **unified storage** approach:

```
┌──────────────────────────────────────────────────────────────┐
│                    Unified Storage                            │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ ShardedStore (DashMap<Key, VersionedValue>)              ││
│  │   ├── Key: { namespace, type_tag, user_key }             ││
│  │   └── VersionedValue: { value, version, timestamp, ttl } ││
│  └──────────────────────────────────────────────────────────┘│
│                           ↑                                   │
│     ┌─────────┬─────────┬─────────┬─────────┬─────────┐      │
│     │ KV      │ Event   │ State   │ Trace   │ Run     │      │
│     │ 0x01    │ 0x02    │ 0x03    │ 0x04    │ 0x05    │      │
│     └─────────┴─────────┴─────────┴─────────┴─────────┘      │
└──────────────────────────────────────────────────────────────┘
```

**Key characteristics:**
- **Single storage backend** (ShardedStore using DashMap)
- **TypeTag discrimination** (0x01-0x05 for existing primitives)
- **Namespace-based isolation** (tenant/app/agent/run_id)
- **Global version counter** (AtomicU64 for all operations)
- **Key ordering** (namespace → type_tag → user_key)

### M5 Current Design (PROBLEMATIC)

The M5 EPICS document proposes a **separate storage layer**:

```rust
// FROM M5_EPICS.md Story #235
pub struct JsonStore {
    docs: DashMap<(RunId, JsonDocId), Arc<JsonDoc>>,  // SEPARATE STORAGE!
}
```

### Issues

1. **Parallel Storage**: Creates a second DashMap separate from ShardedStore
2. **Version Isolation**: Document versions are separate from global version counter
3. **Snapshot Inconsistency**: Cannot use existing SnapshotView abstraction
4. **No TypeTag**: Bypasses the unified key system
5. **WAL Incompatibility**: Existing WAL uses Key-based entries

### Recommendation: Use Unified Storage

```rust
// RECOMMENDED APPROACH
// Add TypeTag for JSON
pub enum TypeTag {
    KV = 0x01,
    Event = 0x02,
    State = 0x03,
    Trace = 0x04,
    Run = 0x05,
    Vector = 0x10,
    Json = 0x11,  // NEW: JSON documents
}

// JSON documents stored in unified storage
impl Key {
    /// Create a JSON document key
    pub fn new_json(namespace: Namespace, doc_id: &JsonDocId) -> Self {
        Self::new(namespace, TypeTag::Json, doc_id.as_bytes().to_vec())
    }

    /// Create a JSON metadata key (for document index)
    pub fn new_json_meta(namespace: Namespace, doc_id: &JsonDocId) -> Self {
        let key_data = format!("__meta__{}", doc_id);
        Self::new(namespace, TypeTag::Json, key_data.into_bytes())
    }
}
```

**Benefits of unified approach:**
- Uses existing ShardedStore (no new storage layer)
- Participates in global version numbering
- Works with existing SnapshotView for isolation
- Fits existing WAL entry patterns
- Enables cross-primitive queries (scan by run)

---

## 2. Value Type Analysis

### Existing Value Enum

```rust
// FROM crates/core/src/value.rs
pub enum Value {
    Null,
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Map(std::collections::HashMap<String, Value>),
}
```

### M5 Proposed JsonValue

```rust
// FROM M5_EPICS.md Story #226
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(JsonNumber),    // DIFFERENT: Split Int/Float
    String(String),
    Array(Vec<JsonValue>),
    Object(IndexMap<String, JsonValue>),  // DIFFERENT: IndexMap for order
}

pub enum JsonNumber {
    Int(i64),
    Float(f64),
}
```

### Key Differences

| Feature | Existing Value | M5 JsonValue | Impact |
|---------|---------------|--------------|--------|
| Number types | Separate I64/F64 | Combined JsonNumber | Minor conversion |
| Object key order | HashMap (unordered) | IndexMap (ordered) | **Semantic difference** |
| Binary data | Bytes(Vec<u8>) | Not supported | JSON doesn't have binary |
| Recursion | Value references Value | JsonValue references JsonValue | Type separation |

### Recommendation: Evaluate Trade-offs

**Option A: Keep JsonValue Separate (Current M5 Design)**

Pros:
- IndexMap preserves insertion order (important for JSON compliance)
- Clean separation between database values and JSON documents
- No conversion overhead for JSON-heavy workloads

Cons:
- Conversion required for cross-primitive transactions
- Two similar types to maintain
- Users must understand when to use which

**Option B: Extend Existing Value Enum**

```rust
pub enum Value {
    Null,
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Map(std::collections::HashMap<String, Value>),
    OrderedMap(IndexMap<String, Value>),  // NEW: For JSON objects
}
```

Pros:
- Single value type across all primitives
- No conversion for cross-primitive transactions
- Simpler mental model

Cons:
- Adds variant to existing enum (potentially breaking)
- HashMap vs IndexMap confusion
- Binary (Bytes) allowed in "JSON" documents

**Option C: JsonValue with Bidirectional Conversion (RECOMMENDED)**

Keep JsonValue separate but provide seamless conversion:

```rust
impl From<Value> for JsonValue {
    fn from(v: Value) -> Self {
        match v {
            Value::Null => JsonValue::Null,
            Value::Bool(b) => JsonValue::Bool(b),
            Value::I64(n) => JsonValue::Number(JsonNumber::Int(n)),
            Value::F64(n) => JsonValue::Number(JsonNumber::Float(n)),
            Value::String(s) => JsonValue::String(s),
            Value::Bytes(b) => JsonValue::String(base64::encode(b)), // Encode binary
            Value::Array(a) => JsonValue::Array(a.into_iter().map(Into::into).collect()),
            Value::Map(m) => JsonValue::Object(m.into_iter().map(|(k, v)| (k, v.into())).collect()),
        }
    }
}

impl From<JsonValue> for Value {
    fn from(v: JsonValue) -> Self {
        match v {
            JsonValue::Null => Value::Null,
            JsonValue::Bool(b) => Value::Bool(b),
            JsonValue::Number(JsonNumber::Int(n)) => Value::I64(n),
            JsonValue::Number(JsonNumber::Float(n)) => Value::F64(n),
            JsonValue::String(s) => Value::String(s),
            JsonValue::Array(a) => Value::Array(a.into_iter().map(Into::into).collect()),
            JsonValue::Object(m) => Value::Map(m.into_iter().map(|(k, v)| (k, v.into())).collect()),
        }
    }
}
```

---

## 3. Transaction Integration Analysis

### Existing TransactionContext

```rust
// FROM crates/concurrency/src/transaction.rs
pub struct TransactionContext {
    pub txn_id: u64,
    pub run_id: RunId,
    pub start_version: u64,

    snapshot: Option<Box<dyn SnapshotView>>,

    pub read_set: HashMap<Key, u64>,      // Key → version read
    pub write_set: HashMap<Key, Value>,   // Key → value to write
    pub delete_set: HashSet<Key>,         // Keys to delete
    pub cas_set: Vec<CASOperation>,       // CAS operations

    pub status: TransactionStatus,
}
```

### M5 Proposed Transaction State

```rust
// FROM M5_EPICS.md Story #244-246
pub struct JsonTransactionState {
    read_set: Option<Vec<JsonReadEntry>>,   // DIFFERENT STRUCTURE
    write_set: Option<Vec<JsonWriteEntry>>, // DIFFERENT STRUCTURE
}

pub struct JsonReadEntry {
    pub run_id: RunId,
    pub doc_id: JsonDocId,
    pub path: JsonPath,        // PATH-BASED (not key-based)
    pub version_at_read: u64,
}

pub struct JsonWriteEntry {
    pub run_id: RunId,
    pub doc_id: JsonDocId,
    pub patch: JsonPatch,      // PATCH-BASED (not value-based)
    pub resulting_version: u64,
}
```

### Key Differences

| Feature | Existing | M5 Proposed | Impact |
|---------|----------|-------------|--------|
| Read tracking | Key → version | (doc_id, path) → version | **Path-level granularity** |
| Write tracking | Key → Value | (doc_id, patch) | **Patch-based** |
| Storage key | Key (unified) | JsonDocId (separate) | **Incompatible** |
| Validation | Key version check | Path overlap + version | **Different algorithm** |

### Issues with Current M5 Design

1. **Separate State Structure**: JsonTransactionState is parallel to TransactionContext
2. **No Unified Key**: JsonDocId doesn't map to Key
3. **Path-Based Granularity**: Fundamentally different from key-based tracking
4. **Validation Complexity**: Two validation paths (key-based + path-based)

### Recommendation: Extend TransactionContext

```rust
// RECOMMENDED: Extend existing TransactionContext
impl TransactionContext {
    // JSON operations store in existing sets, using unified Key

    /// Read a JSON document (tracks in read_set)
    pub fn json_get(&mut self, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> {
        let key = Key::new_json(Namespace::for_run(self.run_id), doc_id);

        // Get full document from existing infrastructure
        let doc = self.get(&key)?;

        // Apply path traversal
        match doc {
            Some(Value::String(json_str)) => {
                let json_doc: JsonDoc = serde_json::from_str(&json_str)?;
                Ok(get_at_path(&json_doc.value, path).cloned())
            }
            _ => Ok(None),
        }
    }

    /// Write a JSON document (buffers in write_set)
    pub fn json_set(&mut self, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<()> {
        let key = Key::new_json(Namespace::for_run(self.run_id), doc_id);

        // Get current doc (or create new)
        let mut doc = match self.get(&key)? {
            Some(Value::String(json_str)) => serde_json::from_str(&json_str)?,
            _ => JsonDoc::new(*doc_id, JsonValue::Object(IndexMap::new())),
        };

        // Apply patch
        set_at_path(&mut doc.value, path, value)?;
        doc.touch();

        // Store as serialized Value::String (like other primitives)
        let serialized = serde_json::to_string(&doc)?;
        self.put(key, Value::String(serialized))
    }
}
```

**Benefits:**
- Uses existing read_set/write_set
- Works with existing validation
- No separate transaction state
- Cross-primitive atomicity "just works"

**Trade-off:**
- Document-level conflict detection (not path-level)
- Full document reads/writes (not patch-based WAL)

### Path-Level Conflict Detection (Optional Enhancement)

If path-level conflict detection is critical, add metadata tracking:

```rust
// Optional: Path tracking for fine-grained conflict detection
pub struct TransactionContext {
    // ... existing fields ...

    /// JSON path reads for fine-grained conflict detection
    /// Maps (Key, JsonPath) → version
    /// Only populated for JSON documents when path-level tracking is needed
    json_path_reads: Option<HashMap<(Key, JsonPath), u64>>,
}
```

---

## 4. WAL Integration Analysis

### Existing WAL Entry Types

```rust
// FROM crates/durability/src/wal.rs
pub enum WALEntry {
    BeginTxn { txn_id, run_id, timestamp },
    Write { run_id, key: Key, value: Value, version },
    Delete { run_id, key: Key, version },
    CommitTxn { txn_id, run_id },
    AbortTxn { txn_id, run_id },
    Checkpoint { snapshot_id, version, active_runs },
}
```

### M5 Proposed JSON WAL Entries

```rust
// FROM M5_EPICS.md Story #240
pub const WAL_JSON_CREATE: u8 = 0x20;
pub const WAL_JSON_SET: u8 = 0x21;
pub const WAL_JSON_DELETE: u8 = 0x22;
pub const WAL_JSON_DELETE_DOC: u8 = 0x23;

pub enum JsonWalEntry {
    Create { run_id, doc_id, value, version },
    Set { run_id, doc_id, path, value, version },  // PATCH-BASED
    Delete { run_id, doc_id, path, version },       // PATCH-BASED
    DeleteDoc { run_id, doc_id },
}
```

### Analysis

**Aligned aspects:**
- Entry type constants (0x20-0x23) don't conflict
- All entries include run_id
- Version tracking for idempotent replay

**Potential issues:**
- Separate enum (JsonWalEntry) vs extending WALEntry
- Patch-based entries don't use Key

### Recommendation: Two Options

**Option A: Extend WALEntry (Simpler)**

```rust
pub enum WALEntry {
    // ... existing variants ...

    /// JSON document create
    JsonCreate {
        run_id: RunId,
        key: Key,  // Uses unified Key
        value: Value,  // Serialized JsonDoc
        version: u64,
    },

    /// JSON patch (set at path)
    JsonPatch {
        run_id: RunId,
        key: Key,
        path: JsonPath,
        patch_value: Value,  // The value to set at path
        version: u64,
    },

    /// JSON delete at path
    JsonDeletePath {
        run_id: RunId,
        key: Key,
        path: JsonPath,
        version: u64,
    },
}
```

**Option B: Separate JsonWalEntry (M5 Current Design)**

Keep JsonWalEntry separate, but ensure replay integrates properly:

```rust
impl WAL {
    pub fn append_json(&mut self, entry: &JsonWalEntry) -> Result<u64> {
        // Convert to bytes with 0x20-0x23 type prefix
        let encoded = encode_json_entry(entry)?;
        self.append_raw(&encoded)
    }
}

impl Recovery {
    fn replay_entry(&self, entry_type: u8, data: &[u8]) -> Result<()> {
        match entry_type {
            0x01..=0x06 => self.replay_standard(entry_type, data),
            0x20..=0x23 => self.replay_json(entry_type, data),
            _ => Err(Error::UnknownEntryType(entry_type)),
        }
    }
}
```

---

## 5. API Pattern Analysis

### Existing Primitive API Pattern

All existing primitives follow the **stateless facade pattern**:

```rust
pub struct KVStore {
    db: Arc<Database>,  // Only state: reference to database
}

impl KVStore {
    pub fn new(db: Arc<Database>) -> Self { ... }

    // Fast path (implicit transaction)
    pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>> { ... }

    // Single-op with implicit transaction
    pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()> { ... }

    // Explicit transaction
    pub fn transaction<F>(&self, run_id: &RunId, f: F) -> Result<T> { ... }
}
```

**Key characteristics:**
- **Stateless facade**: Only holds Arc<Database>
- **Run-scoped operations**: All operations take RunId
- **Fast path reads**: Direct snapshot read, bypassing transaction
- **Explicit transaction**: Closure-based for multi-op atomicity
- **Extension trait**: Enables cross-primitive transactions

### M5 Proposed API

```rust
// FROM M5_EPICS.md Story #235
pub struct JsonStore {
    docs: DashMap<(RunId, JsonDocId), Arc<JsonDoc>>,  // STATEFUL!
}

impl JsonStore {
    pub fn create(&self, run_id: RunId, doc_id: JsonDocId, value: JsonValue) -> Result<()> { ... }
    pub fn get(&self, run_id: RunId, doc_id: JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> { ... }
    pub fn set(&self, run_id: RunId, doc_id: JsonDocId, path: &JsonPath, value: JsonValue) -> Result<u64> { ... }
}
```

### Issues

1. **Stateful**: Holds its own DashMap (not stateless facade)
2. **No Database reference**: Can't participate in Database transactions
3. **No extension trait**: Can't do cross-primitive operations
4. **Different parameter order**: (run_id, doc_id) vs (&run_id, key)
5. **Different return types**: Returns `u64` version vs `Result<()>`

### Recommendation: Align with Existing Pattern

```rust
// RECOMMENDED: Stateless facade pattern
pub struct JsonStore {
    db: Arc<Database>,
}

impl JsonStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    // === Fast Path (Implicit Transaction) ===

    /// Get value at path (fast path)
    pub fn get(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = Key::new_json(Namespace::for_run(*run_id), doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc: JsonDoc = deserialize_doc(&vv.value)?;
                Ok(get_at_path(&doc.value, path).cloned())
            }
            None => Ok(None),
        }
    }

    // === Single Operations (Implicit Transaction) ===

    /// Create document
    pub fn create(&self, run_id: &RunId, doc_id: &JsonDocId, value: JsonValue) -> Result<()> {
        self.db.transaction(*run_id, |txn| {
            let key = Key::new_json(Namespace::for_run(*run_id), doc_id);

            // Check doesn't exist
            if txn.get(&key)?.is_some() {
                return Err(Error::AlreadyExists(format!("Document {}", doc_id)));
            }

            let doc = JsonDoc::new(*doc_id, value);
            let serialized = serialize_doc(&doc)?;
            txn.put(key, Value::String(serialized))
        })
    }

    /// Set value at path
    pub fn set(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<()> {
        self.db.transaction(*run_id, |txn| {
            txn.json_set(doc_id, path, value)
        })
    }

    // === Explicit Transaction ===

    pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut JsonTransaction<'_>) -> Result<T>,
    {
        self.db.transaction(*run_id, |txn| {
            let mut json_txn = JsonTransaction { txn, run_id: *run_id };
            f(&mut json_txn)
        })
    }
}

/// Extension trait for cross-primitive transactions
pub trait JsonStoreExt {
    fn json_get(&mut self, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>>;
    fn json_set(&mut self, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<()>;
    fn json_delete_path(&mut self, doc_id: &JsonDocId, path: &JsonPath) -> Result<()>;
}

impl JsonStoreExt for TransactionContext {
    // ... implementations ...
}
```

---

## 6. Serialization Strategy

### Existing Pattern

All existing primitives serialize complex structures as JSON strings in `Value::String`:

```rust
// FROM crates/primitives/src/event_log.rs
fn to_stored_value<T: Serialize>(v: &T) -> Value {
    match serde_json::to_string(v) {
        Ok(s) => Value::String(s),
        Err(_) => Value::Null,
    }
}
```

### M5 Proposed: MessagePack

```rust
// FROM M5_EPICS.md Story #239
impl JsonStore {
    pub fn serialize_doc(doc: &JsonDoc) -> Result<Vec<u8>, JsonStoreError> {
        rmp_serde::to_vec(doc)
            .map_err(|e| JsonStoreError::Serialization(e.to_string()))
    }
}
```

### Recommendation: Align with Existing Pattern

For consistency, use JSON serialization (can switch to MessagePack later):

```rust
// Store as JSON string (consistent with other primitives)
pub fn serialize_doc(doc: &JsonDoc) -> Result<Value, JsonStoreError> {
    serde_json::to_string(doc)
        .map(Value::String)
        .map_err(|e| JsonStoreError::Serialization(e.to_string()))
}

// OR if MessagePack efficiency is critical, use Value::Bytes
pub fn serialize_doc_msgpack(doc: &JsonDoc) -> Result<Value, JsonStoreError> {
    rmp_serde::to_vec(doc)
        .map(Value::Bytes)
        .map_err(|e| JsonStoreError::Serialization(e.to_string()))
}
```

---

## 7. Summary of Recommendations

### High Priority (Must Fix)

| Issue | Current M5 Design | Recommended Change |
|-------|-------------------|-------------------|
| Storage | Separate DashMap | Use unified ShardedStore with TypeTag::Json |
| Key System | JsonDocId only | Use Key::new_json(namespace, doc_id) |
| Facade Pattern | Stateful JsonStore | Stateless with Arc<Database> |
| Transaction | Separate JsonTransactionState | Extend TransactionContext |

### Medium Priority (Should Align)

| Issue | Current M5 Design | Recommended Change |
|-------|-------------------|-------------------|
| Value Type | Separate JsonValue | Keep separate with From<Value>/Into<Value> |
| Serialization | MessagePack | JSON string (consistent) or Value::Bytes for msgpack |
| API Parameter Order | (run_id, doc_id) | (&run_id, doc_id) - reference like other primitives |
| Extension Trait | Missing | Add JsonStoreExt for cross-primitive ops |

### Low Priority (Can Keep)

| Issue | Current M5 Design | Notes |
|-------|-------------------|-------|
| WAL Entry Types | 0x20-0x23 separate | OK - doesn't conflict, can extend later |
| Path Operations | Dedicated functions | Good - JSON-specific logic |
| Validation | Path-based conflict | Good - unique to JSON semantics |

---

## 8. Proposed Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              User Code                                   │
└────────────────────────────────┬────────────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                         Primitive Facades                                │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌────────┐│
│  │ KVStore │ │EventLog │ │StateCell│ │  Trace  │ │RunIndex │ │JsonStore││
│  │(facade) │ │(facade) │ │(facade) │ │(facade) │ │(facade) │ │(facade)││
│  └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ └────┬────┘ └───┬────┘│
│       │           │           │           │           │          │     │
│       └───────────┴───────────┴─────┬─────┴───────────┴──────────┘     │
│                                     │                                   │
│                                     ▼                                   │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                         Database                                 │   │
│  │  ┌──────────────────────────────────────────────────────────┐   │   │
│  │  │               TransactionContext                          │   │   │
│  │  │  ┌──────────────────────────────────────────────────────┐│   │   │
│  │  │  │ read_set:   HashMap<Key, u64>                        ││   │   │
│  │  │  │ write_set:  HashMap<Key, Value>                      ││   │   │
│  │  │  │ delete_set: HashSet<Key>                             ││   │   │
│  │  │  │ cas_set:    Vec<CASOperation>                        ││   │   │
│  │  │  └──────────────────────────────────────────────────────┘│   │   │
│  │  └──────────────────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                     │                                   │
│                                     ▼                                   │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                    ShardedStore (DashMap)                        │   │
│  │  ┌───────────────────────────────────────────────────────────┐  │   │
│  │  │ Key { namespace, type_tag, user_key } → VersionedValue    │  │   │
│  │  │                                                            │  │   │
│  │  │ TypeTag::KV    (0x01) → KV data                           │  │   │
│  │  │ TypeTag::Event (0x02) → Event data                        │  │   │
│  │  │ TypeTag::State (0x03) → State data                        │  │   │
│  │  │ TypeTag::Trace (0x04) → Trace data                        │  │   │
│  │  │ TypeTag::Run   (0x05) → Run data                          │  │   │
│  │  │ TypeTag::Json  (0x11) → JSON documents (NEW)              │  │   │
│  │  └───────────────────────────────────────────────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                     │                                   │
│                                     ▼                                   │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                           WAL                                    │   │
│  │  BeginTxn | Write | Delete | CommitTxn | JsonPatch | ...        │   │
│  └─────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 9. Next Steps

1. **Update M5_IMPLEMENTATION_PLAN.md** with alignment changes
2. **Revise Epic 26** (Core Types) to add TypeTag::Json
3. **Revise Epic 28** (JsonStore Core) to use stateless facade pattern
4. **Revise Epic 30** (Transaction Integration) to extend TransactionContext
5. **Add JsonStoreExt** extension trait to extensions.rs
6. **Update GitHub issues** to reflect new architecture

---

## Appendix A: File Changes Required

| File | Change Type | Description |
|------|-------------|-------------|
| `crates/core/src/types.rs` | MODIFY | Add TypeTag::Json (0x11) |
| `crates/core/src/types.rs` | ADD | Key::new_json() helper |
| `crates/primitives/src/json_store.rs` | CREATE | Stateless facade |
| `crates/primitives/src/extensions.rs` | MODIFY | Add JsonStoreExt trait |
| `crates/concurrency/src/transaction.rs` | MODIFY | Add json_* methods |
| `crates/durability/src/wal.rs` | MODIFY | Add JSON WAL entries (optional) |

## Appendix B: Backward Compatibility

All changes maintain backward compatibility:
- New TypeTag value doesn't affect existing primitives
- Existing WAL entries unchanged
- TransactionContext extensions are additive
- No breaking changes to existing APIs
