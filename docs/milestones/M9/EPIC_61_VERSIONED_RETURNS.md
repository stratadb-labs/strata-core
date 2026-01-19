# Epic 61: Versioned Returns

**Goal**: Wrap all read returns in Versioned<T>, all writes return Version

**Dependencies**: Epic 60 (Core Types)

---

## Implementation Order

> **Do not convert all 7 primitives in one pass.**

This epic spans multiple implementation phases. Follow this order:

| Phase | Stories | Primitives |
|-------|---------|------------|
| Phase 2 | #466, #467 | KV, EventLog |
| Phase 3 | #468, #469 | StateCell, TraceStore |
| Phase 4 | #470, #471, #472 | JsonStore, VectorStore, RunIndex |

Each phase should be completed and tested before moving to the next.
Conformance tests for each primitive should pass before proceeding.

---

## Scope

- Update KVStore to return Versioned<Value> on reads, Version on writes
- Update EventLog to return Versioned<Event> on reads, Version on writes
- Update StateCell to return Versioned<StateValue> on reads, Version on writes
- Update TraceStore to return Versioned<Trace> on reads, Versioned<TraceId> on record
- Update JsonStore to return Versioned<JsonValue> on reads, Version on writes
- Update VectorStore to return Versioned<VectorEntry> on reads, Version on writes
- Update RunIndex to return Versioned<RunMetadata> on reads, Version on writes

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #466 | KVStore Versioned Returns | CRITICAL |
| #467 | EventLog Versioned Returns | CRITICAL |
| #468 | StateCell Versioned Returns | CRITICAL |
| #469 | TraceStore Versioned Returns | CRITICAL |
| #470 | JsonStore Versioned Returns | CRITICAL |
| #471 | VectorStore Versioned Returns | CRITICAL |
| #472 | RunIndex Versioned Returns | CRITICAL |

---

## Story #466: KVStore Versioned Returns

**File**: `crates/primitives/src/kv_store.rs`

**Deliverable**: KVStore returns Versioned<Value> on reads, Version on writes

### Current API (Before)

```rust
impl KVStore {
    pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>>;
    pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()>;
    pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool>;
    pub fn exists(&self, run_id: &RunId, key: &str) -> Result<bool>;
}
```

### New API (After)

```rust
use crate::{Versioned, Version, Timestamp};

impl KVStore {
    /// Get a value by key
    ///
    /// Returns Versioned<Value> if found, None if not found.
    /// The version indicates when this value was last written.
    pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>> {
        let key = Key::new_kv(self.namespace(run_id), key);

        match self.store.get(&key)? {
            Some(entry) => {
                let value = Value::from_bytes(&entry.value)?;
                Ok(Some(Versioned::new(
                    value,
                    Version::TxnId(entry.version),
                    Timestamp::from_micros(entry.timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// Put a value by key (upsert semantics)
    ///
    /// Returns the version created by this write.
    pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version> {
        let key = Key::new_kv(self.namespace(run_id), key);
        let version = self.store.put(&key, value.to_bytes()?)?;
        Ok(Version::TxnId(version))
    }

    /// Delete a key
    ///
    /// Returns true if the key existed and was deleted.
    /// Note: Delete doesn't return a version because the entity no longer exists.
    pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool> {
        let key = Key::new_kv(self.namespace(run_id), key);
        self.store.delete(&key)
    }

    /// Check if a key exists
    ///
    /// This is a read operation that doesn't return version info.
    /// Use get() if you need the version.
    pub fn exists(&self, run_id: &RunId, key: &str) -> Result<bool> {
        let key = Key::new_kv(self.namespace(run_id), key);
        self.store.exists(&key)
    }

    /// Get multiple keys at once
    ///
    /// Returns a map of key -> Versioned<Value> for found keys.
    pub fn get_many(
        &self,
        run_id: &RunId,
        keys: &[&str],
    ) -> Result<HashMap<String, Versioned<Value>>> {
        let mut results = HashMap::new();
        for key in keys {
            if let Some(versioned) = self.get(run_id, key)? {
                results.insert(key.to_string(), versioned);
            }
        }
        Ok(results)
    }

    // === Migration Helpers ===

    /// Get value only, discarding version (DEPRECATED)
    #[deprecated(since = "0.9.0", note = "Use get() and access .value directly")]
    pub fn get_value(&self, run_id: &RunId, key: &str) -> Result<Option<Value>> {
        Ok(self.get(run_id, key)?.map(|v| v.value))
    }
}
```

### Acceptance Criteria

- [ ] `get()` returns `Result<Option<Versioned<Value>>>`
- [ ] `put()` returns `Result<Version>`
- [ ] `delete()` returns `Result<bool>` (unchanged)
- [ ] `exists()` returns `Result<bool>` (unchanged)
- [ ] Version uses `Version::TxnId` variant
- [ ] Timestamp populated from storage entry
- [ ] Migration helper `get_value()` deprecated but available
- [ ] All existing tests updated

---

## Story #467: EventLog Versioned Returns

**File**: `crates/primitives/src/event_log.rs`

**Deliverable**: EventLog returns Versioned<Event> on reads, Version on writes

### New API

```rust
impl EventLog {
    /// Append an event to the log
    ///
    /// Returns the sequence number (as Version::Sequence) of the appended event.
    pub fn append(
        &self,
        run_id: &RunId,
        event_type: &str,
        payload: Value,
    ) -> Result<Version> {
        let sequence = self.inner_append(run_id, event_type, payload)?;
        Ok(Version::Sequence(sequence))
    }

    /// Read an event by sequence number
    ///
    /// Returns Versioned<Event> if found.
    pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Versioned<Event>>> {
        match self.inner_read(run_id, sequence)? {
            Some(event) => {
                Ok(Some(Versioned::new(
                    event.clone(),
                    Version::Sequence(sequence),
                    Timestamp::from_micros(event.timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// Read a range of events
    ///
    /// Returns Vec<Versioned<Event>> for the range [start, end).
    pub fn read_range(
        &self,
        run_id: &RunId,
        start: u64,
        end: u64,
    ) -> Result<Vec<Versioned<Event>>> {
        let events = self.inner_read_range(run_id, start, end)?;
        Ok(events
            .into_iter()
            .enumerate()
            .map(|(i, event)| {
                let seq = start + i as u64;
                Versioned::new(
                    event.clone(),
                    Version::Sequence(seq),
                    Timestamp::from_micros(event.timestamp),
                )
            })
            .collect())
    }

    /// Get the current sequence number (next write position)
    ///
    /// Returns Version::Sequence representing the next available sequence.
    pub fn current_sequence(&self, run_id: &RunId) -> Result<Version> {
        let seq = self.inner_current_sequence(run_id)?;
        Ok(Version::Sequence(seq))
    }

    /// Read all events for a run
    pub fn read_all(&self, run_id: &RunId) -> Result<Vec<Versioned<Event>>> {
        let current = self.inner_current_sequence(run_id)?;
        if current == 0 {
            return Ok(vec![]);
        }
        self.read_range(run_id, 0, current)
    }
}
```

### Acceptance Criteria

- [ ] `append()` returns `Result<Version>` (Version::Sequence)
- [ ] `read()` returns `Result<Option<Versioned<Event>>>`
- [ ] `read_range()` returns `Result<Vec<Versioned<Event>>>`
- [ ] `current_sequence()` returns `Result<Version>`
- [ ] Sequence numbers map to Version::Sequence
- [ ] Event timestamp used for Versioned timestamp
- [ ] All existing tests updated

---

## Story #468: StateCell Versioned Returns

**File**: `crates/primitives/src/state_cell.rs`

**Deliverable**: StateCell returns Versioned<StateValue> on reads, Version on writes

### New API

```rust
impl StateCell {
    /// Read the current state
    ///
    /// Returns Versioned<StateValue> with the counter-based version.
    pub fn read(&self, run_id: &RunId, name: &str) -> Result<Option<Versioned<StateValue>>> {
        match self.inner_read(run_id, name)? {
            Some((value, version, timestamp)) => {
                Ok(Some(Versioned::new(
                    value,
                    Version::Counter(version),
                    Timestamp::from_micros(timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// Set the state value
    ///
    /// Returns the new version (counter).
    pub fn set(&self, run_id: &RunId, name: &str, value: Value) -> Result<Version> {
        let version = self.inner_set(run_id, name, value)?;
        Ok(Version::Counter(version))
    }

    /// Compare-and-swap: update only if version matches
    ///
    /// Returns the new version if successful.
    /// Returns error with current version if version mismatch.
    pub fn cas(
        &self,
        run_id: &RunId,
        name: &str,
        expected: Version,
        value: Value,
    ) -> Result<Version> {
        let expected_counter = match expected {
            Version::Counter(c) => c,
            _ => return Err(StateError::InvalidVersion { expected }.into()),
        };

        let new_version = self.inner_cas(run_id, name, expected_counter, value)?;
        Ok(Version::Counter(new_version))
    }

    /// Initialize a state cell (fails if already exists)
    ///
    /// Returns the initial version (1).
    pub fn init(&self, run_id: &RunId, name: &str, value: Value) -> Result<Version> {
        let version = self.inner_init(run_id, name, value)?;
        Ok(Version::Counter(version))
    }

    /// Delete a state cell
    pub fn delete(&self, run_id: &RunId, name: &str) -> Result<bool> {
        self.inner_delete(run_id, name)
    }

    /// Check if a state cell exists
    pub fn exists(&self, run_id: &RunId, name: &str) -> Result<bool> {
        self.inner_exists(run_id, name)
    }
}
```

### Acceptance Criteria

- [ ] `read()` returns `Result<Option<Versioned<StateValue>>>`
- [ ] `set()` returns `Result<Version>` (Version::Counter)
- [ ] `cas()` takes Version, returns `Result<Version>`
- [ ] `init()` returns `Result<Version>`
- [ ] `delete()` returns `Result<bool>` (unchanged)
- [ ] `exists()` returns `Result<bool>` (unchanged)
- [ ] CAS validates expected version is Counter variant
- [ ] All existing tests updated

---

## Story #469: TraceStore Versioned Returns

**File**: `crates/primitives/src/trace_store.rs`

**Deliverable**: TraceStore returns Versioned<Trace> on reads, Versioned<TraceId> on record

### New API

```rust
impl TraceStore {
    /// Record a new trace
    ///
    /// Returns Versioned<TraceId> containing the new trace ID and version.
    /// The TraceId is part of the versioned result because it's generated
    /// by the system and the caller needs it.
    pub fn record(
        &self,
        run_id: &RunId,
        trace_type: TraceType,
        content: Value,
        tags: Vec<String>,
    ) -> Result<Versioned<TraceId>> {
        let (trace_id, version, timestamp) = self.inner_record(
            run_id,
            trace_type,
            content,
            tags,
        )?;

        Ok(Versioned::new(
            trace_id,
            Version::TxnId(version),
            Timestamp::from_micros(timestamp),
        ))
    }

    /// Read a trace by ID
    ///
    /// Returns Versioned<Trace> if found.
    pub fn read(&self, run_id: &RunId, trace_id: &TraceId) -> Result<Option<Versioned<Trace>>> {
        match self.inner_read(run_id, trace_id)? {
            Some((trace, version, timestamp)) => {
                Ok(Some(Versioned::new(
                    trace,
                    Version::TxnId(version),
                    Timestamp::from_micros(timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// List traces matching criteria
    ///
    /// Returns Vec<Versioned<Trace>>.
    pub fn list(
        &self,
        run_id: &RunId,
        filter: TraceFilter,
    ) -> Result<Vec<Versioned<Trace>>> {
        let traces = self.inner_list(run_id, filter)?;
        Ok(traces
            .into_iter()
            .map(|(trace, version, timestamp)| {
                Versioned::new(
                    trace,
                    Version::TxnId(version),
                    Timestamp::from_micros(timestamp),
                )
            })
            .collect())
    }

    /// Check if a trace exists
    pub fn exists(&self, run_id: &RunId, trace_id: &TraceId) -> Result<bool> {
        self.inner_exists(run_id, trace_id)
    }
}
```

### Acceptance Criteria

- [ ] `record()` returns `Result<Versioned<TraceId>>`
- [ ] `read()` returns `Result<Option<Versioned<Trace>>>`
- [ ] `list()` returns `Result<Vec<Versioned<Trace>>>`
- [ ] `exists()` returns `Result<bool>` (unchanged)
- [ ] TraceId wrapped in Versioned on record (caller needs the ID)
- [ ] All existing tests updated

---

## Story #470: JsonStore Versioned Returns

**File**: `crates/primitives/src/json_store.rs`

**Deliverable**: JsonStore returns Versioned<JsonValue> on reads, Version on writes

### New API

```rust
impl JsonStore {
    /// Create a new JSON document
    ///
    /// Returns the initial version.
    pub fn create(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        value: JsonValue,
    ) -> Result<Version> {
        let version = self.inner_create(run_id, doc_id, value)?;
        Ok(Version::TxnId(version))
    }

    /// Get entire document
    ///
    /// Returns Versioned<JsonValue> if found.
    pub fn get(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<Option<Versioned<JsonValue>>> {
        match self.inner_get(run_id, doc_id)? {
            Some((value, version, timestamp)) => {
                Ok(Some(Versioned::new(
                    value,
                    Version::TxnId(version),
                    Timestamp::from_micros(timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// Get value at a path within the document
    ///
    /// Returns Versioned<JsonValue> for the path if found.
    /// The version is the document version, not a path-specific version.
    pub fn get_path(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<Versioned<JsonValue>>> {
        match self.inner_get_path(run_id, doc_id, path)? {
            Some((value, version, timestamp)) => {
                Ok(Some(Versioned::new(
                    value,
                    Version::TxnId(version),
                    Timestamp::from_micros(timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// Set value at a path
    ///
    /// Returns the new document version.
    pub fn set(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<Version> {
        let version = self.inner_set(run_id, doc_id, path, value)?;
        Ok(Version::TxnId(version))
    }

    /// Delete a document
    pub fn delete(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool> {
        self.inner_delete(run_id, doc_id)
    }

    /// Check if a document exists
    pub fn exists(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool> {
        self.inner_exists(run_id, doc_id)
    }

    /// Apply a JSON patch
    ///
    /// Returns the new document version.
    pub fn patch(
        &self,
        run_id: &RunId,
        doc_id: &JsonDocId,
        patch: JsonPatch,
    ) -> Result<Version> {
        let version = self.inner_patch(run_id, doc_id, patch)?;
        Ok(Version::TxnId(version))
    }
}
```

### Acceptance Criteria

- [ ] `create()` returns `Result<Version>`
- [ ] `get()` returns `Result<Option<Versioned<JsonValue>>>`
- [ ] `get_path()` returns `Result<Option<Versioned<JsonValue>>>`
- [ ] `set()` returns `Result<Version>`
- [ ] `delete()` returns `Result<bool>` (unchanged)
- [ ] `exists()` returns `Result<bool>` (unchanged)
- [ ] `patch()` returns `Result<Version>`
- [ ] All existing tests updated

---

## Story #471: VectorStore Versioned Returns

**File**: `crates/primitives/src/vector/store.rs`

**Deliverable**: VectorStore returns Versioned<VectorEntry> on reads, Version on writes

### New API

```rust
impl VectorStore {
    /// Upsert vectors into a collection
    ///
    /// Returns the version of this operation.
    pub fn upsert(
        &self,
        run_id: &RunId,
        collection: &str,
        entries: Vec<VectorEntry>,
    ) -> Result<Version> {
        let version = self.inner_upsert(run_id, collection, entries)?;
        Ok(Version::TxnId(version))
    }

    /// Get a vector by key
    ///
    /// Returns Versioned<VectorEntry> if found.
    pub fn get(
        &self,
        run_id: &RunId,
        collection: &str,
        key: &str,
    ) -> Result<Option<Versioned<VectorEntry>>> {
        match self.inner_get(run_id, collection, key)? {
            Some((entry, version, timestamp)) => {
                Ok(Some(Versioned::new(
                    entry,
                    Version::TxnId(version),
                    Timestamp::from_micros(timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// Delete a vector by key
    pub fn delete(&self, run_id: &RunId, collection: &str, key: &str) -> Result<bool> {
        self.inner_delete(run_id, collection, key)
    }

    /// Search for similar vectors
    ///
    /// Note: Search results don't include version info per match.
    /// This is intentional - search returns ranked results, not versioned reads.
    pub fn search(
        &self,
        run_id: &RunId,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<VectorMatch>> {
        self.inner_search(run_id, collection, query, k, filter)
    }

    /// Create a collection
    ///
    /// Returns the version of this operation.
    pub fn create_collection(
        &self,
        run_id: &RunId,
        name: &str,
        config: VectorConfig,
    ) -> Result<Version> {
        let version = self.inner_create_collection(run_id, name, config)?;
        Ok(Version::TxnId(version))
    }

    /// Get collection info
    ///
    /// Returns Versioned<CollectionInfo> if found.
    pub fn get_collection(
        &self,
        run_id: &RunId,
        name: &str,
    ) -> Result<Option<Versioned<CollectionInfo>>> {
        match self.inner_get_collection(run_id, name)? {
            Some((info, version, timestamp)) => {
                Ok(Some(Versioned::new(
                    info,
                    Version::TxnId(version),
                    Timestamp::from_micros(timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// Delete a collection
    pub fn delete_collection(&self, run_id: &RunId, name: &str) -> Result<bool> {
        self.inner_delete_collection(run_id, name)
    }

    /// Check if a collection exists
    pub fn collection_exists(&self, run_id: &RunId, name: &str) -> Result<bool> {
        self.inner_collection_exists(run_id, name)
    }
}
```

### Acceptance Criteria

- [ ] `upsert()` returns `Result<Version>`
- [ ] `get()` returns `Result<Option<Versioned<VectorEntry>>>`
- [ ] `delete()` returns `Result<bool>` (unchanged)
- [ ] `search()` returns `Result<Vec<VectorMatch>>` (unchanged - search is not versioned read)
- [ ] `create_collection()` returns `Result<Version>`
- [ ] `get_collection()` returns `Result<Option<Versioned<CollectionInfo>>>`
- [ ] `delete_collection()` returns `Result<bool>` (unchanged)
- [ ] All existing tests updated

---

## Story #472: RunIndex Versioned Returns

**File**: `crates/primitives/src/run_index.rs`

**Deliverable**: RunIndex returns Versioned<RunMetadata> on reads, Version on writes

### New API

```rust
impl RunIndex {
    /// Create a new run
    ///
    /// Returns the version of this operation.
    pub fn create_run(&self, run_id: &RunId, metadata: RunMetadata) -> Result<Version> {
        let version = self.inner_create_run(run_id, metadata)?;
        Ok(Version::TxnId(version))
    }

    /// Get run metadata
    ///
    /// Returns Versioned<RunMetadata> if found.
    pub fn get_run(&self, run_id: &RunId) -> Result<Option<Versioned<RunMetadata>>> {
        match self.inner_get_run(run_id)? {
            Some((metadata, version, timestamp)) => {
                Ok(Some(Versioned::new(
                    metadata,
                    Version::TxnId(version),
                    Timestamp::from_micros(timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// Update run status
    ///
    /// Returns the new version.
    pub fn update_status(&self, run_id: &RunId, status: RunStatus) -> Result<Version> {
        let version = self.inner_update_status(run_id, status)?;
        Ok(Version::TxnId(version))
    }

    /// Transition run to completed
    ///
    /// Returns the new version.
    pub fn complete_run(&self, run_id: &RunId) -> Result<Version> {
        self.update_status(run_id, RunStatus::Completed)
    }

    /// Delete a run and all its data
    pub fn delete_run(&self, run_id: &RunId) -> Result<bool> {
        self.inner_delete_run(run_id)
    }

    /// Check if a run exists
    pub fn run_exists(&self, run_id: &RunId) -> Result<bool> {
        self.inner_run_exists(run_id)
    }

    /// List all runs
    ///
    /// Returns Vec<Versioned<RunMetadata>>.
    pub fn list_runs(&self) -> Result<Vec<Versioned<RunMetadata>>> {
        let runs = self.inner_list_runs()?;
        Ok(runs
            .into_iter()
            .map(|(metadata, version, timestamp)| {
                Versioned::new(
                    metadata,
                    Version::TxnId(version),
                    Timestamp::from_micros(timestamp),
                )
            })
            .collect())
    }

    /// List runs by status
    pub fn list_runs_by_status(&self, status: RunStatus) -> Result<Vec<Versioned<RunMetadata>>> {
        let runs = self.inner_list_runs_by_status(status)?;
        Ok(runs
            .into_iter()
            .map(|(metadata, version, timestamp)| {
                Versioned::new(
                    metadata,
                    Version::TxnId(version),
                    Timestamp::from_micros(timestamp),
                )
            })
            .collect())
    }
}
```

### Acceptance Criteria

- [ ] `create_run()` returns `Result<Version>`
- [ ] `get_run()` returns `Result<Option<Versioned<RunMetadata>>>`
- [ ] `update_status()` returns `Result<Version>`
- [ ] `complete_run()` returns `Result<Version>`
- [ ] `delete_run()` returns `Result<bool>` (unchanged)
- [ ] `run_exists()` returns `Result<bool>` (unchanged)
- [ ] `list_runs()` returns `Result<Vec<Versioned<RunMetadata>>>`
- [ ] All existing tests updated

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // === KVStore Tests ===

    #[test]
    fn test_kv_get_returns_versioned() {
        let db = test_database();
        let run_id = RunId::new("test-run");

        // Put returns version
        let put_version = db.kv().put(&run_id, "key", Value::from("value")).unwrap();
        assert!(put_version.is_txn());

        // Get returns versioned
        let result = db.kv().get(&run_id, "key").unwrap();
        assert!(result.is_some());

        let versioned = result.unwrap();
        assert_eq!(versioned.value, Value::from("value"));
        assert!(versioned.version.as_u64() > 0);
        assert!(versioned.timestamp.as_micros() > 0);
    }

    #[test]
    fn test_kv_put_increments_version() {
        let db = test_database();
        let run_id = RunId::new("test-run");

        let v1 = db.kv().put(&run_id, "key", Value::from("v1")).unwrap();
        let v2 = db.kv().put(&run_id, "key", Value::from("v2")).unwrap();

        assert!(v2 > v1, "Version should increment on update");
    }

    // === EventLog Tests ===

    #[test]
    fn test_event_append_returns_sequence_version() {
        let db = test_database();
        let run_id = RunId::new("test-run");

        let version = db.events().append(&run_id, "test", json!({})).unwrap();
        assert!(version.is_sequence());
        assert_eq!(version.as_u64(), 0); // First event
    }

    #[test]
    fn test_event_read_returns_versioned() {
        let db = test_database();
        let run_id = RunId::new("test-run");

        db.events().append(&run_id, "test", json!({"data": 1})).unwrap();

        let result = db.events().read(&run_id, 0).unwrap();
        assert!(result.is_some());

        let versioned = result.unwrap();
        assert_eq!(versioned.version, Version::Sequence(0));
    }

    // === StateCell Tests ===

    #[test]
    fn test_state_returns_counter_version() {
        let db = test_database();
        let run_id = RunId::new("test-run");

        let v1 = db.state().set(&run_id, "cell", Value::from(1)).unwrap();
        assert!(v1.is_counter());
        assert_eq!(v1.as_u64(), 1); // First version is 1

        let v2 = db.state().set(&run_id, "cell", Value::from(2)).unwrap();
        assert_eq!(v2.as_u64(), 2); // Incremented
    }

    #[test]
    fn test_state_cas_with_version() {
        let db = test_database();
        let run_id = RunId::new("test-run");

        let v1 = db.state().set(&run_id, "cell", Value::from(1)).unwrap();

        // CAS with correct version succeeds
        let v2 = db.state().cas(&run_id, "cell", v1, Value::from(2)).unwrap();
        assert!(v2 > v1);

        // CAS with wrong version fails
        let result = db.state().cas(&run_id, "cell", v1, Value::from(3));
        assert!(result.is_err());
    }

    // === TraceStore Tests ===

    #[test]
    fn test_trace_record_returns_versioned_id() {
        let db = test_database();
        let run_id = RunId::new("test-run");

        let result = db.traces().record(
            &run_id,
            TraceType::Action,
            json!({"action": "test"}),
            vec!["tag1".to_string()],
        ).unwrap();

        // We get a Versioned<TraceId>
        let trace_id = result.value;
        let version = result.version;

        assert!(version.is_txn());

        // Can read it back
        let read_result = db.traces().read(&run_id, &trace_id).unwrap();
        assert!(read_result.is_some());
    }

    // === Version Consistency Tests ===

    #[test]
    fn test_read_version_matches_write_version() {
        let db = test_database();
        let run_id = RunId::new("test-run");

        let write_version = db.kv().put(&run_id, "key", Value::from("value")).unwrap();
        let read_result = db.kv().get(&run_id, "key").unwrap().unwrap();

        assert_eq!(read_result.version, write_version);
    }

    #[test]
    fn test_versioned_map_preserves_metadata() {
        let db = test_database();
        let run_id = RunId::new("test-run");

        db.kv().put(&run_id, "key", Value::from("hello")).unwrap();
        let versioned = db.kv().get(&run_id, "key").unwrap().unwrap();

        let original_version = versioned.version;
        let original_timestamp = versioned.timestamp;

        let mapped = versioned.map(|v| v.to_string());

        assert_eq!(mapped.version, original_version);
        assert_eq!(mapped.timestamp, original_timestamp);
    }
}
```

---

## Migration Guide

### Before (Old API)

```rust
// Old: Get returns Option<Value>
let value = kv.get(&run_id, "key")?.unwrap();

// Old: Put returns ()
kv.put(&run_id, "key", value)?;
```

### After (New API)

```rust
// New: Get returns Option<Versioned<Value>>
let versioned = kv.get(&run_id, "key")?.unwrap();
let value = versioned.value;
let version = versioned.version;
let when = versioned.timestamp;

// New: Put returns Version
let version = kv.put(&run_id, "key", value)?;
```

### Migration Path

1. Update imports: `use crate::{Versioned, Version, Timestamp};`
2. Update get calls: `.unwrap()` → `.unwrap().value` or use versioned
3. Update put calls: Add version capture if needed
4. Remove uses of deprecated `get_value()` method

---

## Files Modified

| File | Action |
|------|--------|
| `crates/primitives/src/kv_store.rs` | MODIFY - Versioned returns |
| `crates/primitives/src/event_log.rs` | MODIFY - Versioned returns |
| `crates/primitives/src/state_cell.rs` | MODIFY - Versioned returns |
| `crates/primitives/src/trace_store.rs` | MODIFY - Versioned returns |
| `crates/primitives/src/json_store.rs` | MODIFY - Versioned returns |
| `crates/primitives/src/vector/store.rs` | MODIFY - Versioned returns |
| `crates/primitives/src/run_index.rs` | MODIFY - Versioned returns |
| `tests/**/*.rs` | MODIFY - Update all tests |
