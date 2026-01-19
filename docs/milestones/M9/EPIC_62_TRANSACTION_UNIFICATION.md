# Epic 62: Transaction Unification

**Goal**: Unified TransactionOps trait covering all primitives

**Dependencies**: Epic 60 (Core Types)

---

## Implementation Order

> **Do not convert all 7 primitives in one pass.**

This epic spans multiple implementation phases. Follow this order:

| Phase | Stories | Primitives |
|-------|---------|------------|
| Phase 2 | #473, #474, #475 | Trait definition + KV + EventLog |
| Phase 3 | #476 | StateCell + TraceStore |
| Phase 4 | #477 | JsonStore + VectorStore |
| Phase 5 | #478 | RunHandle pattern (finalize) |

The trait definition (#473) should include method signatures for all primitives upfront,
but implementations are wired incrementally as primitives are converted.

---

## Scope

- Define TransactionOps trait with methods for all 7 primitives
- Implement Transaction type that implements TransactionOps
- Define RunHandle pattern for scoped primitive access
- Ensure all primitives can participate in cross-primitive transactions
- Consistent read (`&self`) and write (`&mut self`) semantics

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #473 | TransactionOps Trait Definition | FOUNDATION |
| #474 | KV Operations in TransactionOps | CRITICAL |
| #475 | Event Operations in TransactionOps | CRITICAL |
| #476 | State/Trace Operations in TransactionOps | CRITICAL |
| #477 | Json/Vector Operations in TransactionOps | CRITICAL |
| #478 | RunHandle Pattern Implementation | HIGH |

---

## Story #473: TransactionOps Trait Definition

**File**: `crates/engine/src/transaction_ops.rs` (NEW)

**Deliverable**: Unified trait defining all primitive operations within a transaction

### Implementation

```rust
use crate::{
    EntityRef, Versioned, Version, Timestamp, RunId, StrataError,
    Value, Event, StateValue, Trace, TraceId, TraceType,
    JsonValue, JsonPath, JsonDocId,
    VectorEntry, VectorMatch, CollectionId, VectorId, VectorConfig, MetadataFilter,
    RunMetadata, RunStatus,
};

/// Operations available within a transaction
///
/// This trait expresses Invariant 3: Everything is Transactional.
/// Every primitive's operations are accessible through this trait,
/// enabling cross-primitive atomic operations.
///
/// ## Design Principles
///
/// 1. **Reads are `&self`**: Read operations never modify state
/// 2. **Writes are `&mut self`**: Write operations require exclusive access
/// 3. **All operations return `Result<T, StrataError>`**: Consistent error handling
/// 4. **All reads return `Versioned<T>`**: Version information is never lost
/// 5. **All writes return `Version`**: Every mutation produces a version
///
/// ## Usage
///
/// ```rust
/// db.transaction(&run_id, |txn| {
///     // Read from KV
///     let config = txn.kv_get("config")?;
///
///     // Write to Event
///     let event_version = txn.event_append("config_read", json!({}))?;
///
///     // Update State
///     txn.state_set("last_event", Value::from(event_version.as_u64()))?;
///
///     Ok(())
/// })?;
/// ```
pub trait TransactionOps {
    // =========================================================================
    // KV Operations
    // =========================================================================

    /// Get a KV entry by key
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError>;

    /// Put a KV entry (upsert semantics)
    fn kv_put(&mut self, key: &str, value: Value) -> Result<Version, StrataError>;

    /// Delete a KV entry
    fn kv_delete(&mut self, key: &str) -> Result<bool, StrataError>;

    /// Check if a KV entry exists
    fn kv_exists(&self, key: &str) -> Result<bool, StrataError>;

    /// List keys matching a prefix
    fn kv_list(&self, prefix: &str) -> Result<Vec<String>, StrataError>;

    // =========================================================================
    // Event Operations
    // =========================================================================

    /// Append an event to the log
    fn event_append(
        &mut self,
        event_type: &str,
        payload: Value,
    ) -> Result<Version, StrataError>;

    /// Read an event by sequence number
    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>, StrataError>;

    /// Read a range of events [start, end)
    fn event_range(
        &self,
        start: u64,
        end: u64,
    ) -> Result<Vec<Versioned<Event>>, StrataError>;

    /// Get current sequence number
    fn event_current_sequence(&self) -> Result<u64, StrataError>;

    // =========================================================================
    // State Operations
    // =========================================================================

    /// Read a state cell
    fn state_read(&self, name: &str) -> Result<Option<Versioned<StateValue>>, StrataError>;

    /// Set a state cell value
    fn state_set(&mut self, name: &str, value: Value) -> Result<Version, StrataError>;

    /// Compare-and-swap a state cell
    fn state_cas(
        &mut self,
        name: &str,
        expected: Version,
        value: Value,
    ) -> Result<Version, StrataError>;

    /// Delete a state cell
    fn state_delete(&mut self, name: &str) -> Result<bool, StrataError>;

    /// Check if a state cell exists
    fn state_exists(&self, name: &str) -> Result<bool, StrataError>;

    // =========================================================================
    // Trace Operations
    // =========================================================================

    /// Record a trace entry
    fn trace_record(
        &mut self,
        trace_type: TraceType,
        content: Value,
        tags: Vec<String>,
    ) -> Result<Versioned<TraceId>, StrataError>;

    /// Read a trace by ID
    fn trace_read(&self, trace_id: &TraceId) -> Result<Option<Versioned<Trace>>, StrataError>;

    /// Check if a trace exists
    fn trace_exists(&self, trace_id: &TraceId) -> Result<bool, StrataError>;

    // =========================================================================
    // Json Operations
    // =========================================================================

    /// Create a JSON document
    fn json_create(
        &mut self,
        doc_id: &JsonDocId,
        value: JsonValue,
    ) -> Result<Version, StrataError>;

    /// Get an entire JSON document
    fn json_get(&self, doc_id: &JsonDocId) -> Result<Option<Versioned<JsonValue>>, StrataError>;

    /// Get a value at a path within a JSON document
    fn json_get_path(
        &self,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<Versioned<JsonValue>>, StrataError>;

    /// Set a value at a path within a JSON document
    fn json_set(
        &mut self,
        doc_id: &JsonDocId,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<Version, StrataError>;

    /// Delete a JSON document
    fn json_delete(&mut self, doc_id: &JsonDocId) -> Result<bool, StrataError>;

    /// Check if a JSON document exists
    fn json_exists(&self, doc_id: &JsonDocId) -> Result<bool, StrataError>;

    // =========================================================================
    // Vector Operations
    // =========================================================================

    /// Upsert vectors into a collection
    fn vector_upsert(
        &mut self,
        collection: &str,
        entries: Vec<VectorEntry>,
    ) -> Result<Version, StrataError>;

    /// Get a vector by key
    fn vector_get(
        &self,
        collection: &str,
        key: &str,
    ) -> Result<Option<Versioned<VectorEntry>>, StrataError>;

    /// Delete a vector
    fn vector_delete(&mut self, collection: &str, key: &str) -> Result<bool, StrataError>;

    /// Search for similar vectors
    fn vector_search(
        &self,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<VectorMatch>, StrataError>;

    /// Check if a vector exists
    fn vector_exists(&self, collection: &str, key: &str) -> Result<bool, StrataError>;

    // =========================================================================
    // Run Operations (Limited - runs are meta-level)
    // =========================================================================

    /// Get run metadata (the current run)
    fn run_metadata(&self) -> Result<Versioned<RunMetadata>, StrataError>;

    /// Update run status
    fn run_update_status(&mut self, status: RunStatus) -> Result<Version, StrataError>;
}
```

### Acceptance Criteria

- [ ] TransactionOps trait defined with all primitive operations
- [ ] Reads are `&self`, writes are `&mut self`
- [ ] All methods return `Result<T, StrataError>`
- [ ] KV: get, put, delete, exists, list
- [ ] Event: append, read, range, current_sequence
- [ ] State: read, set, cas, delete, exists
- [ ] Trace: record, read, exists
- [ ] Json: create, get, get_path, set, delete, exists
- [ ] Vector: upsert, get, delete, search, exists
- [ ] Run: metadata, update_status

---

## Story #474: KV Operations in TransactionOps

**File**: `crates/engine/src/transaction.rs`

**Deliverable**: Implement KV operations for Transaction type

### Implementation

```rust
impl TransactionOps for Transaction<'_> {
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError> {
        let full_key = Key::new_kv(self.namespace(), key);

        // Check write set first (read-your-writes)
        if let Some(pending) = self.write_set.get(&full_key) {
            return match pending {
                PendingWrite::Put { value, version } => {
                    Ok(Some(Versioned::new(
                        value.clone(),
                        Version::TxnId(*version),
                        Timestamp::now(),
                    )))
                }
                PendingWrite::Delete => Ok(None),
            };
        }

        // Check snapshot
        match self.snapshot.get(&full_key)? {
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

    fn kv_put(&mut self, key: &str, value: Value) -> Result<Version, StrataError> {
        let full_key = Key::new_kv(self.namespace(), key);
        let version = self.next_version();

        // Track read for conflict detection
        self.read_set.insert(full_key.clone());

        // Buffer write
        self.write_set.insert(
            full_key,
            PendingWrite::Put {
                value,
                version,
            },
        );

        Ok(Version::TxnId(version))
    }

    fn kv_delete(&mut self, key: &str) -> Result<bool, StrataError> {
        let full_key = Key::new_kv(self.namespace(), key);

        // Check if exists (for return value)
        let existed = self.kv_exists(key)?;

        // Track read for conflict detection
        self.read_set.insert(full_key.clone());

        // Buffer delete
        self.write_set.insert(full_key, PendingWrite::Delete);

        Ok(existed)
    }

    fn kv_exists(&self, key: &str) -> Result<bool, StrataError> {
        let full_key = Key::new_kv(self.namespace(), key);

        // Check write set first
        if let Some(pending) = self.write_set.get(&full_key) {
            return match pending {
                PendingWrite::Put { .. } => Ok(true),
                PendingWrite::Delete => Ok(false),
            };
        }

        // Check snapshot
        self.snapshot.exists(&full_key)
    }

    fn kv_list(&self, prefix: &str) -> Result<Vec<String>, StrataError> {
        let full_prefix = Key::new_kv(self.namespace(), prefix);

        // Get keys from snapshot
        let mut keys: Vec<String> = self.snapshot
            .scan_prefix(&full_prefix)?
            .map(|k| k.user_key().to_string())
            .collect();

        // Adjust for pending writes
        for (key, pending) in &self.write_set {
            if key.starts_with(&full_prefix) {
                let user_key = key.user_key().to_string();
                match pending {
                    PendingWrite::Put { .. } => {
                        if !keys.contains(&user_key) {
                            keys.push(user_key);
                        }
                    }
                    PendingWrite::Delete => {
                        keys.retain(|k| k != &user_key);
                    }
                }
            }
        }

        keys.sort();
        Ok(keys)
    }
}
```

### Acceptance Criteria

- [ ] `kv_get` reads from write set first (read-your-writes)
- [ ] `kv_get` falls back to snapshot
- [ ] `kv_put` buffers write, returns version
- [ ] `kv_delete` buffers delete, returns existed
- [ ] `kv_exists` checks write set then snapshot
- [ ] `kv_list` merges snapshot and pending writes
- [ ] Read set tracked for conflict detection

---

## Story #475: Event Operations in TransactionOps

**File**: `crates/engine/src/transaction.rs`

**Deliverable**: Implement Event operations for Transaction type

### Implementation

```rust
impl TransactionOps for Transaction<'_> {
    fn event_append(
        &mut self,
        event_type: &str,
        payload: Value,
    ) -> Result<Version, StrataError> {
        // Events are append-only, so we allocate the next sequence
        let sequence = self.allocate_event_sequence();

        let event = Event {
            event_type: event_type.to_string(),
            payload,
            timestamp: Timestamp::now().as_micros(),
            sequence,
        };

        // Buffer the event append
        self.pending_events.push(event);

        Ok(Version::Sequence(sequence))
    }

    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>, StrataError> {
        // Check pending events first
        for event in &self.pending_events {
            if event.sequence == sequence {
                return Ok(Some(Versioned::new(
                    event.clone(),
                    Version::Sequence(sequence),
                    Timestamp::from_micros(event.timestamp),
                )));
            }
        }

        // Check snapshot
        let key = Key::new_event(self.namespace(), sequence);
        match self.snapshot.get(&key)? {
            Some(entry) => {
                let event = Event::from_bytes(&entry.value)?;
                Ok(Some(Versioned::new(
                    event.clone(),
                    Version::Sequence(sequence),
                    Timestamp::from_micros(event.timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    fn event_range(
        &self,
        start: u64,
        end: u64,
    ) -> Result<Vec<Versioned<Event>>, StrataError> {
        let mut results = Vec::new();

        for seq in start..end {
            if let Some(versioned) = self.event_read(seq)? {
                results.push(versioned);
            }
        }

        Ok(results)
    }

    fn event_current_sequence(&self) -> Result<u64, StrataError> {
        // Base sequence from snapshot
        let base = self.snapshot_event_sequence()?;

        // Add pending events
        Ok(base + self.pending_events.len() as u64)
    }
}
```

### Acceptance Criteria

- [ ] `event_append` allocates sequence, buffers event
- [ ] `event_read` checks pending events first
- [ ] `event_range` reads range of events
- [ ] `event_current_sequence` includes pending events
- [ ] Events have Version::Sequence

---

## Story #476: State/Trace Operations in TransactionOps

**File**: `crates/engine/src/transaction.rs`

**Deliverable**: Implement State and Trace operations for Transaction type

### Implementation

```rust
impl TransactionOps for Transaction<'_> {
    // === State Operations ===

    fn state_read(&self, name: &str) -> Result<Option<Versioned<StateValue>>, StrataError> {
        let key = Key::new_state(self.namespace(), name);

        // Check write set first
        if let Some(pending) = self.write_set.get(&key) {
            return match pending {
                PendingWrite::Put { value, version } => {
                    Ok(Some(Versioned::new(
                        StateValue::from(value.clone()),
                        Version::Counter(*version),
                        Timestamp::now(),
                    )))
                }
                PendingWrite::Delete => Ok(None),
            };
        }

        // Check snapshot
        match self.snapshot.get(&key)? {
            Some(entry) => {
                let state = StateValue::from_bytes(&entry.value)?;
                Ok(Some(Versioned::new(
                    state,
                    Version::Counter(entry.version),
                    Timestamp::from_micros(entry.timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    fn state_set(&mut self, name: &str, value: Value) -> Result<Version, StrataError> {
        let key = Key::new_state(self.namespace(), name);

        // Get current version to increment
        let current_version = self.state_read(name)?
            .map(|v| v.version.as_u64())
            .unwrap_or(0);
        let new_version = current_version + 1;

        self.read_set.insert(key.clone());
        self.write_set.insert(
            key,
            PendingWrite::Put {
                value,
                version: new_version,
            },
        );

        Ok(Version::Counter(new_version))
    }

    fn state_cas(
        &mut self,
        name: &str,
        expected: Version,
        value: Value,
    ) -> Result<Version, StrataError> {
        let current = self.state_read(name)?;

        match current {
            Some(versioned) => {
                if versioned.version != expected {
                    return Err(StrataError::VersionConflict {
                        entity_ref: EntityRef::state(self.run_id().clone(), name),
                        expected,
                        actual: versioned.version,
                    });
                }
                self.state_set(name, value)
            }
            None if expected == Version::Counter(0) => {
                // Creating new cell with expected version 0
                self.state_set(name, value)
            }
            None => {
                Err(StrataError::NotFound {
                    entity_ref: EntityRef::state(self.run_id().clone(), name),
                })
            }
        }
    }

    fn state_delete(&mut self, name: &str) -> Result<bool, StrataError> {
        let key = Key::new_state(self.namespace(), name);
        let existed = self.state_exists(name)?;

        self.read_set.insert(key.clone());
        self.write_set.insert(key, PendingWrite::Delete);

        Ok(existed)
    }

    fn state_exists(&self, name: &str) -> Result<bool, StrataError> {
        Ok(self.state_read(name)?.is_some())
    }

    // === Trace Operations ===

    fn trace_record(
        &mut self,
        trace_type: TraceType,
        content: Value,
        tags: Vec<String>,
    ) -> Result<Versioned<TraceId>, StrataError> {
        let trace_id = self.allocate_trace_id();
        let version = self.next_version();
        let timestamp = Timestamp::now();

        let trace = Trace {
            id: trace_id.clone(),
            trace_type,
            content,
            tags,
            timestamp: timestamp.as_micros(),
        };

        let key = Key::new_trace(self.namespace(), &trace_id);
        self.write_set.insert(
            key,
            PendingWrite::Put {
                value: Value::from_bytes(&trace.to_bytes()?),
                version,
            },
        );

        Ok(Versioned::new(trace_id, Version::TxnId(version), timestamp))
    }

    fn trace_read(&self, trace_id: &TraceId) -> Result<Option<Versioned<Trace>>, StrataError> {
        let key = Key::new_trace(self.namespace(), trace_id);

        // Check write set first
        if let Some(pending) = self.write_set.get(&key) {
            return match pending {
                PendingWrite::Put { value, version } => {
                    let trace = Trace::from_bytes(&value.to_bytes()?)?;
                    Ok(Some(Versioned::new(
                        trace.clone(),
                        Version::TxnId(*version),
                        Timestamp::from_micros(trace.timestamp),
                    )))
                }
                PendingWrite::Delete => Ok(None),
            };
        }

        // Check snapshot
        match self.snapshot.get(&key)? {
            Some(entry) => {
                let trace = Trace::from_bytes(&entry.value)?;
                Ok(Some(Versioned::new(
                    trace.clone(),
                    Version::TxnId(entry.version),
                    Timestamp::from_micros(trace.timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    fn trace_exists(&self, trace_id: &TraceId) -> Result<bool, StrataError> {
        Ok(self.trace_read(trace_id)?.is_some())
    }
}
```

### Acceptance Criteria

- [ ] `state_read` checks write set, returns Versioned<StateValue>
- [ ] `state_set` increments counter version
- [ ] `state_cas` validates expected version
- [ ] `state_delete` buffers delete
- [ ] `state_exists` checks existence
- [ ] `trace_record` allocates ID, returns Versioned<TraceId>
- [ ] `trace_read` returns Versioned<Trace>
- [ ] `trace_exists` checks existence

---

## Story #477: Json/Vector Operations in TransactionOps

**File**: `crates/engine/src/transaction.rs`

**Deliverable**: Implement Json and Vector operations for Transaction type

### Implementation

```rust
impl TransactionOps for Transaction<'_> {
    // === Json Operations ===

    fn json_create(
        &mut self,
        doc_id: &JsonDocId,
        value: JsonValue,
    ) -> Result<Version, StrataError> {
        let key = Key::new_json(self.namespace(), doc_id);

        // Check if already exists
        if self.json_exists(doc_id)? {
            return Err(StrataError::InvalidOperation {
                entity_ref: EntityRef::json(self.run_id().clone(), doc_id.clone()),
                reason: "Document already exists".to_string(),
            });
        }

        let version = self.next_version();
        self.write_set.insert(
            key,
            PendingWrite::Put {
                value: Value::from_json(&value)?,
                version,
            },
        );

        Ok(Version::TxnId(version))
    }

    fn json_get(&self, doc_id: &JsonDocId) -> Result<Option<Versioned<JsonValue>>, StrataError> {
        let key = Key::new_json(self.namespace(), doc_id);

        // Check write set
        if let Some(pending) = self.write_set.get(&key) {
            return match pending {
                PendingWrite::Put { value, version } => {
                    let json = value.to_json()?;
                    Ok(Some(Versioned::new(
                        json,
                        Version::TxnId(*version),
                        Timestamp::now(),
                    )))
                }
                PendingWrite::Delete => Ok(None),
            };
        }

        // Check snapshot
        match self.snapshot.get(&key)? {
            Some(entry) => {
                let json = serde_json::from_slice(&entry.value)?;
                Ok(Some(Versioned::new(
                    json,
                    Version::TxnId(entry.version),
                    Timestamp::from_micros(entry.timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    fn json_get_path(
        &self,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<Versioned<JsonValue>>, StrataError> {
        match self.json_get(doc_id)? {
            Some(versioned) => {
                let value_at_path = path.get(&versioned.value);
                Ok(value_at_path.map(|v| Versioned::new(
                    v.clone(),
                    versioned.version,
                    versioned.timestamp,
                )))
            }
            None => Ok(None),
        }
    }

    fn json_set(
        &mut self,
        doc_id: &JsonDocId,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<Version, StrataError> {
        // Get current document
        let current = self.json_get(doc_id)?
            .ok_or_else(|| StrataError::NotFound {
                entity_ref: EntityRef::json(self.run_id().clone(), doc_id.clone()),
            })?;

        // Apply path update
        let mut doc = current.value;
        path.set(&mut doc, value)?;

        // Write back
        let key = Key::new_json(self.namespace(), doc_id);
        let version = self.next_version();
        self.read_set.insert(key.clone());
        self.write_set.insert(
            key,
            PendingWrite::Put {
                value: Value::from_json(&doc)?,
                version,
            },
        );

        Ok(Version::TxnId(version))
    }

    fn json_delete(&mut self, doc_id: &JsonDocId) -> Result<bool, StrataError> {
        let key = Key::new_json(self.namespace(), doc_id);
        let existed = self.json_exists(doc_id)?;

        self.read_set.insert(key.clone());
        self.write_set.insert(key, PendingWrite::Delete);

        Ok(existed)
    }

    fn json_exists(&self, doc_id: &JsonDocId) -> Result<bool, StrataError> {
        Ok(self.json_get(doc_id)?.is_some())
    }

    // === Vector Operations ===

    fn vector_upsert(
        &mut self,
        collection: &str,
        entries: Vec<VectorEntry>,
    ) -> Result<Version, StrataError> {
        // Validate collection exists
        if !self.vector_collection_exists(collection)? {
            return Err(StrataError::NotFound {
                entity_ref: EntityRef::vector(
                    self.run_id().clone(),
                    collection,
                    VectorId::new(0),
                ),
            });
        }

        let version = self.next_version();

        // Buffer each vector write
        for entry in entries {
            let key = Key::new_vector(self.namespace(), collection, &entry.key);
            self.read_set.insert(key.clone());
            self.pending_vectors.push(PendingVectorOp::Upsert {
                collection: collection.to_string(),
                entry,
                version,
            });
        }

        Ok(Version::TxnId(version))
    }

    fn vector_get(
        &self,
        collection: &str,
        key: &str,
    ) -> Result<Option<Versioned<VectorEntry>>, StrataError> {
        // Check pending ops first
        for op in self.pending_vectors.iter().rev() {
            match op {
                PendingVectorOp::Upsert { collection: c, entry, version }
                    if c == collection && entry.key == key =>
                {
                    return Ok(Some(Versioned::new(
                        entry.clone(),
                        Version::TxnId(*version),
                        Timestamp::now(),
                    )));
                }
                PendingVectorOp::Delete { collection: c, key: k }
                    if c == collection && k == key =>
                {
                    return Ok(None);
                }
                _ => {}
            }
        }

        // Check snapshot via vector store
        self.vector_store.get_in_snapshot(
            self.snapshot,
            &self.run_id(),
            collection,
            key,
        )
    }

    fn vector_delete(&mut self, collection: &str, key: &str) -> Result<bool, StrataError> {
        let existed = self.vector_exists(collection, key)?;

        self.pending_vectors.push(PendingVectorOp::Delete {
            collection: collection.to_string(),
            key: key.to_string(),
        });

        Ok(existed)
    }

    fn vector_search(
        &self,
        collection: &str,
        query: &[f32],
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<VectorMatch>, StrataError> {
        // Search is read-only and operates on snapshot + pending
        // For simplicity, this delegates to the vector store with snapshot
        self.vector_store.search_in_snapshot(
            self.snapshot,
            &self.run_id(),
            collection,
            query,
            k,
            filter,
            &self.pending_vectors,
        )
    }

    fn vector_exists(&self, collection: &str, key: &str) -> Result<bool, StrataError> {
        Ok(self.vector_get(collection, key)?.is_some())
    }

    // Helper: check if vector collection exists
    fn vector_collection_exists(&self, collection: &str) -> Result<bool, StrataError> {
        // Check pending collection creates
        for op in &self.pending_collections {
            match op {
                PendingCollectionOp::Create { name, .. } if name == collection => return Ok(true),
                PendingCollectionOp::Delete { name } if name == collection => return Ok(false),
                _ => {}
            }
        }

        // Check snapshot
        self.vector_store.collection_exists_in_snapshot(
            self.snapshot,
            &self.run_id(),
            collection,
        )
    }
}
```

### Acceptance Criteria

- [ ] `json_create` validates non-existence, buffers write
- [ ] `json_get` returns Versioned<JsonValue>
- [ ] `json_get_path` applies path to document
- [ ] `json_set` updates path in document
- [ ] `json_delete` buffers delete
- [ ] `json_exists` checks existence
- [ ] `vector_upsert` validates collection, buffers writes
- [ ] `vector_get` checks pending ops first
- [ ] `vector_delete` buffers delete
- [ ] `vector_search` operates on snapshot + pending
- [ ] `vector_exists` checks existence

---

## Story #478: RunHandle Pattern Implementation

**File**: `crates/engine/src/run_handle.rs` (NEW)

**Deliverable**: RunHandle for scoped primitive access

### Implementation

```rust
use crate::{
    Database, RunId, Transaction, TransactionOps, StrataError,
    KvHandle, EventHandle, StateHandle, TraceHandle, JsonHandle, VectorHandle,
};

/// Handle to a specific run
///
/// Provides scoped access to all primitives within a run.
/// The run_id is bound to this handle, so operations don't need
/// to specify it repeatedly.
///
/// ## Usage
///
/// ```rust
/// let run = db.run("my-run");
///
/// // Access primitives
/// let value = run.kv().get("key")?;
/// run.events().append("event", json!({}))?;
///
/// // Or use transactions
/// run.transaction(|txn| {
///     txn.kv_put("key", value)?;
///     txn.event_append("event", json!({}))?;
///     Ok(())
/// })?;
/// ```
#[derive(Clone)]
pub struct RunHandle {
    db: Arc<Database>,
    run_id: RunId,
}

impl RunHandle {
    /// Create a new RunHandle
    pub(crate) fn new(db: Arc<Database>, run_id: RunId) -> Self {
        Self { db, run_id }
    }

    /// Get the run ID
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    // === Primitive Handles ===

    /// Access the KV primitive for this run
    pub fn kv(&self) -> KvHandle {
        KvHandle::new(self.db.clone(), self.run_id.clone())
    }

    /// Access the Event primitive for this run
    pub fn events(&self) -> EventHandle {
        EventHandle::new(self.db.clone(), self.run_id.clone())
    }

    /// Access the State primitive for this run
    pub fn state(&self) -> StateHandle {
        StateHandle::new(self.db.clone(), self.run_id.clone())
    }

    /// Access the Trace primitive for this run
    pub fn traces(&self) -> TraceHandle {
        TraceHandle::new(self.db.clone(), self.run_id.clone())
    }

    /// Access the Json primitive for this run
    pub fn json(&self) -> JsonHandle {
        JsonHandle::new(self.db.clone(), self.run_id.clone())
    }

    /// Access the Vector primitive for this run
    pub fn vectors(&self) -> VectorHandle {
        VectorHandle::new(self.db.clone(), self.run_id.clone())
    }

    // === Transactions ===

    /// Execute a transaction within this run
    ///
    /// All operations in the closure are atomic. Either all succeed,
    /// or none do (rollback on error).
    pub fn transaction<F, T>(&self, f: F) -> Result<T, StrataError>
    where
        F: FnOnce(&mut dyn TransactionOps) -> Result<T, StrataError>,
    {
        self.db.transaction(&self.run_id, f)
    }

    /// Execute a read-only transaction (faster, no writes allowed)
    pub fn read<F, T>(&self, f: F) -> Result<T, StrataError>
    where
        F: FnOnce(&dyn TransactionOps) -> Result<T, StrataError>,
    {
        self.db.read_transaction(&self.run_id, f)
    }
}

/// KV handle scoped to a run
pub struct KvHandle {
    db: Arc<Database>,
    run_id: RunId,
}

impl KvHandle {
    pub(crate) fn new(db: Arc<Database>, run_id: RunId) -> Self {
        Self { db, run_id }
    }

    pub fn get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError> {
        self.db.kv().get(&self.run_id, key)
    }

    pub fn put(&self, key: &str, value: Value) -> Result<Version, StrataError> {
        self.db.kv().put(&self.run_id, key, value)
    }

    pub fn delete(&self, key: &str) -> Result<bool, StrataError> {
        self.db.kv().delete(&self.run_id, key)
    }

    pub fn exists(&self, key: &str) -> Result<bool, StrataError> {
        self.db.kv().exists(&self.run_id, key)
    }
}

// Similar handles for Event, State, Trace, Json, Vector...
// (abbreviated for space)

/// Event handle scoped to a run
pub struct EventHandle {
    db: Arc<Database>,
    run_id: RunId,
}

impl EventHandle {
    pub(crate) fn new(db: Arc<Database>, run_id: RunId) -> Self {
        Self { db, run_id }
    }

    pub fn append(&self, event_type: &str, payload: Value) -> Result<Version, StrataError> {
        self.db.events().append(&self.run_id, event_type, payload)
    }

    pub fn read(&self, sequence: u64) -> Result<Option<Versioned<Event>>, StrataError> {
        self.db.events().read(&self.run_id, sequence)
    }

    pub fn read_all(&self) -> Result<Vec<Versioned<Event>>, StrataError> {
        self.db.events().read_all(&self.run_id)
    }
}

// ... StateHandle, TraceHandle, JsonHandle, VectorHandle
```

### Database Integration

```rust
impl Database {
    /// Get a handle for an existing run
    pub fn run(&self, run_id: impl Into<RunId>) -> RunHandle {
        RunHandle::new(Arc::new(self.clone()), run_id.into())
    }

    /// Create a new run and return its handle
    pub fn create_run(&self, run_id: impl Into<RunId>) -> Result<RunHandle, StrataError> {
        let run_id = run_id.into();
        self.runs().create_run(&run_id, RunMetadata::default())?;
        Ok(self.run(run_id))
    }

    /// Execute a transaction
    pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T, StrataError>
    where
        F: FnOnce(&mut dyn TransactionOps) -> Result<T, StrataError>,
    {
        let mut txn = self.begin_transaction(run_id)?;
        let result = f(&mut txn)?;
        txn.commit()?;
        Ok(result)
    }
}
```

### Acceptance Criteria

- [ ] RunHandle scopes all operations to a run
- [ ] `kv()`, `events()`, `state()`, `traces()`, `json()`, `vectors()` accessors
- [ ] `transaction()` executes atomic operations
- [ ] `read()` for read-only transactions
- [ ] Database `run()` and `create_run()` methods
- [ ] All handles follow same pattern (db + run_id)

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cross_primitive_transaction() {
        let db = test_database();
        let run = db.create_run("test-run").unwrap();

        // Transaction with multiple primitives
        run.transaction(|txn| {
            // KV
            txn.kv_put("key", Value::from("value"))?;

            // Event
            let event_ver = txn.event_append("test", json!({"data": 1}))?;

            // State
            txn.state_set("last_event", Value::from(event_ver.as_u64()))?;

            // Trace
            let trace = txn.trace_record(
                TraceType::Action,
                json!({"action": "test"}),
                vec![],
            )?;

            // Json
            txn.json_create(&JsonDocId::new("doc"), json!({"version": 1}))?;

            Ok(())
        }).unwrap();

        // Verify all writes committed
        assert!(run.kv().get("key").unwrap().is_some());
        assert!(run.events().read(0).unwrap().is_some());
        assert!(run.state().read("last_event").unwrap().is_some());
    }

    #[test]
    fn test_transaction_rollback() {
        let db = test_database();
        let run = db.create_run("test-run").unwrap();

        // Pre-populate
        run.kv().put("existing", Value::from("original")).unwrap();

        // Transaction that fails
        let result = run.transaction(|txn| {
            txn.kv_put("existing", Value::from("modified"))?;
            txn.kv_put("new_key", Value::from("new_value"))?;

            // Simulate error
            Err(StrataError::InvalidOperation {
                entity_ref: EntityRef::kv(run.run_id().clone(), "test"),
                reason: "Intentional failure".to_string(),
            })
        });

        assert!(result.is_err());

        // Verify rollback
        let existing = run.kv().get("existing").unwrap().unwrap();
        assert_eq!(existing.value, Value::from("original")); // Not modified

        let new_key = run.kv().get("new_key").unwrap();
        assert!(new_key.is_none()); // Not created
    }

    #[test]
    fn test_read_your_writes() {
        let db = test_database();
        let run = db.create_run("test-run").unwrap();

        run.transaction(|txn| {
            // Write
            txn.kv_put("key", Value::from("value"))?;

            // Read back within same transaction
            let read = txn.kv_get("key")?;
            assert!(read.is_some());
            assert_eq!(read.unwrap().value, Value::from("value"));

            Ok(())
        }).unwrap();
    }

    #[test]
    fn test_run_handle_pattern() {
        let db = test_database();
        let run = db.create_run("test-run").unwrap();

        // Use handles (implicit single-operation transactions)
        run.kv().put("key", Value::from("value")).unwrap();
        run.events().append("event", json!({})).unwrap();

        // Use explicit transaction
        run.transaction(|txn| {
            let kv = txn.kv_get("key")?;
            let event = txn.event_read(0)?;

            assert!(kv.is_some());
            assert!(event.is_some());

            Ok(())
        }).unwrap();
    }

    #[test]
    fn test_all_primitives_in_transaction() {
        let db = test_database();
        let run = db.create_run("test-run").unwrap();

        // Pre-create collection for vector
        run.vectors().create_collection("col", VectorConfig::for_minilm()).unwrap();

        // All 7 primitives in one transaction
        run.transaction(|txn| {
            // 1. KV
            txn.kv_put("k", Value::from(1))?;

            // 2. Event
            txn.event_append("e", json!({}))?;

            // 3. State
            txn.state_set("s", Value::from(2))?;

            // 4. Trace
            txn.trace_record(TraceType::Action, json!({}), vec![])?;

            // 5. Json
            txn.json_create(&JsonDocId::new("j"), json!({}))?;

            // 6. Vector
            txn.vector_upsert("col", vec![
                VectorEntry::new("v", vec![0.1; 384], None, VectorId::new(0))
            ])?;

            // 7. Run (status update)
            txn.run_update_status(RunStatus::Active)?;

            Ok(())
        }).unwrap();
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/engine/src/transaction_ops.rs` | CREATE - TransactionOps trait |
| `crates/engine/src/transaction.rs` | MODIFY - Implement TransactionOps |
| `crates/engine/src/run_handle.rs` | CREATE - RunHandle and primitive handles |
| `crates/engine/src/database.rs` | MODIFY - Add run(), create_run(), transaction() |
| `crates/engine/src/lib.rs` | MODIFY - Export new types |
