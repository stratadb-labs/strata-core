# M3 Implementation Plan: Primitives

## Response to M3 Architecture Specification

This document provides the complete implementation plan for M3 (Primitives), building on the M3 Architecture Specification v1.2 and following the structure established in M2.

---

## Key Design Decisions (From Architecture Review)

### 1. ✅ Stateless Facade Pattern (ACCEPTED)

**Decision**: All primitives are logically stateful but operationally stateless.

**Our Approach**:
- Primitives hold `Arc<Database>` reference only
- No in-memory caches or state in primitive instances
- All state lives in UnifiedStore via Database transactions
- Multiple instances of same primitive can coexist safely

**Documented Implications**:
```rust
/// M3 Implementation: Stateless Facade
///
/// DESIGN PRINCIPLE: Primitives maintain semantic state (sequences, indices,
/// metadata) stored in UnifiedStore, but hold no in-process state.
///
/// IMPLICATIONS:
/// - Multiple KVStore instances on same Database are safe
/// - No warm-up or cache invalidation concerns
/// - Idempotent retry works correctly
/// - Replay produces same results
///
/// TRADEOFF:
/// - No local caching (every read hits storage)
/// - Acceptable for M3 (agent workloads, small datasets)
pub struct KVStore {
    db: Arc<Database>,  // Only state is reference to database
}
```

---

### 2. ✅ EventLog Single-Writer-Ordered (ACCEPTED)

**Decision**: EventLog serializes all appends through CAS on metadata key.

**Our Approach**:
- Single metadata key per run stores: `{ next_sequence: u64, head_hash: [u8; 32] }`
- Append operation: read metadata → compute hash → write event + update metadata (atomic)
- Parallel appends serialize through OCC retry on metadata CAS

**Documented Implications**:
```rust
/// M3 Implementation: Single-Writer-Ordered EventLog
///
/// DESIGN PRINCIPLE: All appends to a run's event log are serialized.
///
/// HOW IT WORKS:
/// 1. Begin transaction
/// 2. Read metadata key (gets next_sequence, prev_hash)
/// 3. Compute new event hash
/// 4. Write event key + update metadata key
/// 5. Commit (CAS on metadata ensures serialization)
///
/// IMPLICATION: Parallel append is NOT supported by design.
/// This is intentional—event ordering must be total within a run.
```

---

### 3. ✅ Hash Chaining is Causal, Not Cryptographic (ACCEPTED)

**Decision**: Use non-cryptographic hash (DefaultHasher) in M3, upgrade path to SHA-256 in M4+.

**Our Approach**:
- `[u8; 32]` hash format (future-compatible with SHA-256)
- First 8 bytes from DefaultHasher, rest zero-padded
- Chain provides tamper-evidence within process, not tamper-resistance

**Documented Limitations**:
```rust
/// M3 Implementation: Causal Hash Chain
///
/// WHAT IT PROVIDES:
/// - Tamper-evidence within process boundary
/// - Detection of storage corruption
/// - Verification that events are in order
///
/// WHAT IT DOES NOT PROVIDE:
/// - Cryptographic security
/// - Tamper-resistance at storage level
/// - External anchoring for audit trails
///
/// UPGRADE PATH: M4+ may upgrade to SHA-256 with external anchoring
fn compute_event_hash(event: &Event, prev_hash: &[u8; 32]) -> [u8; 32] {
    // Non-crypto hash, padded to 32 bytes for future SHA-256
}
```

---

### 4. ✅ StateCell with Purity Requirement (ACCEPTED)

**Decision**: Transition closures must be pure functions (may execute multiple times).

**Our Approach**:
- Document purity requirement prominently
- `transition()` signature uses `Fn` (not `FnOnce`) to signal re-execution
- Provide both manual CAS pattern and closure pattern

**Documented Contract**:
```rust
/// M3 Implementation: StateCell Purity Contract
///
/// The `transition()` closure may be called multiple times due to OCC retries.
///
/// REQUIREMENTS:
/// - Pure function of inputs (closure result depends only on &State)
/// - No I/O (no file, network, console operations)
/// - No external mutation (don't modify outside variables)
/// - No irreversible effects (no logging, metrics, API calls)
/// - Idempotent (same input → same output)
///
/// ENFORCEMENT: Cannot enforce at compile time (Rust limitation).
/// Documented requirement, not checked by type system.
pub fn transition<F, T>(&self, run_id: RunId, name: &str, f: F) -> Result<T>
where
    F: Fn(&State) -> Result<(Value, T)>,  // Fn, not FnOnce
```

---

### 5. ✅ TraceStore Performance Warning (ACCEPTED)

**Decision**: Document write amplification (3-4 index entries per trace).

**Our Approach**:
- TraceStore optimized for debuggability, not throughput
- Designed for tens-hundreds of traces per run
- Performance warning in documentation and code comments

---

### 6. ✅ RunIndex Status Transition Validation (ACCEPTED)

**Decision**: Enforce valid status transitions at primitive layer.

**Our Approach**:
- Explicit `is_valid_transition(from, to)` function
- No resurrection (terminal states cannot return to Active)
- Archived is terminal (cannot un-archive)
- Cascading hard delete vs soft archive options

---

## Revised Epic Structure

### Overview: 7 Epics, 36 Stories

| Epic | Name | Stories | Duration | Parallelization |
|------|------|---------|----------|-----------------|
| **Epic 13** | Primitives Foundation | 3 | 1 day | After Story #166 |
| **Epic 14** | KVStore Primitive | 5 | 1 day | After Story #169 |
| **Epic 15** | EventLog Primitive | 6 | 1.5 days | After Story #174 |
| **Epic 16** | StateCell Primitive | 5 | 1 day | After Story #180 |
| **Epic 17** | TraceStore Primitive | 6 | 1.5 days | After Story #185 |
| **Epic 18** | RunIndex Primitive | 6 | 1.5 days | After Story #191 |
| **Epic 19** | Integration & Validation | 5 | 1.5 days | After all primitives |

**Total**: 7 epics, 36 stories, ~9-10 days with 5 Claudes in parallel

---

## Epic 13: Primitives Foundation (3 stories, 1 day)

**Goal**: Core infrastructure and common patterns for all primitives

**Dependencies**: M2 complete (Database, TransactionContext, transactions work)

**Deliverables**:
- Primitives crate structure
- TypeTag extensions (KV=0x01, Event=0x02, State=0x03, Trace=0x04, Run=0x05)
- Key construction helpers
- Transaction extension trait infrastructure

### Story #166: Primitives Crate Setup & TypeTag Extensions (3 hours) 🔴 FOUNDATION
**Blocks**: All M3 stories

**Files**:
- `crates/primitives/Cargo.toml`
- `crates/primitives/src/lib.rs`
- `crates/core/src/types.rs` (TypeTag additions)

**Deliverable**: Crate structure and TypeTag enum

**Implementation**:
```rust
// crates/core/src/types.rs
#[repr(u8)]
pub enum TypeTag {
    // M3 primitives
    KV = 0x01,
    Event = 0x02,
    State = 0x03,
    Trace = 0x04,
    Run = 0x05,

    // Reserved for M6+
    Vector = 0x10,
}

// crates/primitives/src/lib.rs
//! Primitives layer for in-mem
//!
//! Provides five high-level primitives as stateless facades over the Database engine:
//! - KVStore: General-purpose key-value storage
//! - EventLog: Immutable append-only event stream
//! - StateCell: CAS-based versioned cells
//! - TraceStore: Structured reasoning traces
//! - RunIndex: Run lifecycle management

pub mod kv;
pub mod event_log;
pub mod state_cell;
pub mod trace;
pub mod run_index;
pub mod extensions;

pub use kv::KVStore;
pub use event_log::{EventLog, Event};
pub use state_cell::{StateCell, State};
pub use trace::{TraceStore, Trace, TraceType};
pub use run_index::{RunIndex, RunMetadata, RunStatus};
```

**Acceptance Criteria**:
- [ ] `crates/primitives` compiles with correct dependencies
- [ ] TypeTag enum has all 5 primitive values
- [ ] TypeTag values do not conflict with existing types
- [ ] `cargo test` passes for primitives crate

---

### Story #167: Key Construction Helpers (3 hours)
**File**: `crates/core/src/types.rs`

**Deliverable**: Key construction methods for each primitive type

**Implementation**:
```rust
impl Key {
    /// Create KV store key
    pub fn new_kv(namespace: Namespace, user_key: &str) -> Self {
        Self::new(namespace, TypeTag::KV, user_key.as_bytes())
    }

    /// Create Event log key (sequence number as big-endian bytes)
    pub fn new_event(namespace: Namespace, sequence: u64) -> Self {
        Self::new(namespace, TypeTag::Event, &sequence.to_be_bytes())
    }

    /// Create Event log metadata key
    pub fn new_event_meta(namespace: Namespace) -> Self {
        Self::new(namespace, TypeTag::Event, b"__meta__")
    }

    /// Create State cell key
    pub fn new_state(namespace: Namespace, cell_name: &str) -> Self {
        Self::new(namespace, TypeTag::State, cell_name.as_bytes())
    }

    /// Create Trace store key
    pub fn new_trace(namespace: Namespace, trace_id: &str) -> Self {
        Self::new(namespace, TypeTag::Trace, trace_id.as_bytes())
    }

    /// Create Trace index key
    pub fn new_trace_index(namespace: Namespace, index_type: &str, index_value: &str, trace_id: &str) -> Self {
        let key_data = format!("__idx_{}__{}__{}", index_type, index_value, trace_id);
        Self::new(namespace, TypeTag::Trace, key_data.as_bytes())
    }

    /// Create Run index key
    pub fn new_run(namespace: Namespace, run_id: RunId) -> Self {
        Self::new(namespace, TypeTag::Run, run_id.as_bytes())
    }

    /// Create Run index metadata key
    pub fn new_run_index(namespace: Namespace, index_type: &str, index_value: &str, run_id: RunId) -> Self {
        let key_data = format!("__idx_{}__{}__{}", index_type, index_value, run_id);
        Self::new(namespace, TypeTag::Run, key_data.as_bytes())
    }
}
```

**Tests**:
- [ ] Key::new_kv() creates correct key format
- [ ] Key::new_event() encodes sequence as big-endian
- [ ] Key::new_event_meta() creates metadata key
- [ ] Key::new_state() creates correct key format
- [ ] Key::new_trace() creates correct key format
- [ ] Key::new_trace_index() creates index key format
- [ ] Key::new_run() creates correct key format
- [ ] Keys with same inputs are equal
- [ ] Keys sort correctly (lexicographic ordering)

---

### Story #168: Transaction Extension Trait Infrastructure (4 hours)
**File**: `crates/primitives/src/extensions.rs`

**Deliverable**: Extension trait pattern for cross-primitive transactions

**Implementation**:
```rust
//! Transaction extension traits for cross-primitive operations
//!
//! DESIGN PRINCIPLE: Extension traits delegate to primitive internals,
//! they do NOT reimplement logic.

use crate::{KVStore, EventLog, StateCell, TraceStore};
use in_mem_concurrency::TransactionContext;
use in_mem_core::{Value, Result};

/// KV operations within a transaction
pub trait KVStoreExt {
    fn kv_get(&mut self, key: &str) -> Result<Option<Value>>;
    fn kv_put(&mut self, key: &str, value: Value) -> Result<()>;
    fn kv_delete(&mut self, key: &str) -> Result<()>;
}

/// Event log operations within a transaction
pub trait EventLogExt {
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64>;
    fn event_read(&mut self, sequence: u64) -> Result<Option<Event>>;
}

/// State cell operations within a transaction
pub trait StateCellExt {
    fn state_read(&mut self, name: &str) -> Result<Option<State>>;
    fn state_cas(&mut self, name: &str, expected_version: u64, new_value: Value) -> Result<u64>;
    fn state_set(&mut self, name: &str, value: Value) -> Result<u64>;
}

/// Trace store operations within a transaction
pub trait TraceStoreExt {
    fn trace_record(&mut self, trace_type: TraceType) -> Result<String>;
    fn trace_record_child(&mut self, parent_id: &str, trace_type: TraceType) -> Result<String>;
}

// Implementation note: These traits will be implemented in their respective
// primitive story implementations (#173, #179, #184, #190)
```

**Tests**:
- [ ] Extension traits compile
- [ ] Extension traits can be imported from primitives crate

---

## Epic 14: KVStore Primitive (5 stories, 1 day)

**Goal**: General-purpose key-value storage with run isolation

**Dependencies**: Epic 13 complete

**Deliverables**:
- KVStore struct
- Single-operation API (get, put, delete, list)
- Multi-operation API (KVTransaction)
- KVStoreExt transaction extension

### Story #169: KVStore Core Structure (3 hours) 🔴 FOUNDATION
**File**: `crates/primitives/src/kv.rs`

**Deliverable**: KVStore struct and basic infrastructure

**Implementation**:
```rust
use std::sync::Arc;
use in_mem_engine::Database;
use in_mem_core::{RunId, Namespace, Key, Value, Result, TypeTag};

/// General-purpose key-value store primitive
///
/// Stateless facade over Database - all state lives in UnifiedStore.
/// Multiple KVStore instances on same Database are safe.
pub struct KVStore {
    db: Arc<Database>,
}

impl KVStore {
    /// Create new KVStore instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: RunId) -> Namespace {
        // Delegate to run's namespace (tenant/app/agent/run)
        Namespace::for_run(run_id)
    }

    /// Build key for KV operation
    fn key_for(&self, run_id: RunId, user_key: &str) -> Key {
        Key::new_kv(self.namespace_for_run(run_id), user_key)
    }
}
```

**Tests**:
- [ ] KVStore::new() creates instance
- [ ] Multiple KVStore instances can coexist
- [ ] namespace_for_run() returns correct namespace
- [ ] key_for() creates correct key with TypeTag::KV

---

### Story #170: KVStore Single-Operation API (4 hours)
**File**: `crates/primitives/src/kv.rs`

**Deliverable**: get, put, put_with_ttl, delete operations

**Implementation**:
```rust
impl KVStore {
    /// Get a value by key
    pub fn get(&self, run_id: RunId, key: &str) -> Result<Option<Value>> {
        self.db.transaction(run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.get(&storage_key)
        })
    }

    /// Put a value
    pub fn put(&self, run_id: RunId, key: &str, value: Value) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.put(storage_key, value)
        })
    }

    /// Put a value with TTL
    ///
    /// Note: TTL metadata is stored but cleanup is deferred to M4 background tasks
    pub fn put_with_ttl(
        &self,
        run_id: RunId,
        key: &str,
        value: Value,
        ttl: Duration,
    ) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            let expires_at = Timestamp::now().0 + ttl.as_millis() as i64;

            // Store value with expiration metadata
            let value_with_ttl = Value::Map(hashmap! {
                "value".to_string() => value,
                "expires_at".to_string() => Value::I64(expires_at),
            });

            txn.put(storage_key, value_with_ttl)
        })
    }

    /// Delete a key
    pub fn delete(&self, run_id: RunId, key: &str) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.delete(storage_key)
        })
    }
}
```

**Tests**:
- [ ] put() stores value
- [ ] get() retrieves stored value
- [ ] get() returns None for missing key
- [ ] delete() removes key
- [ ] get() returns None after delete()
- [ ] put_with_ttl() stores expiration metadata
- [ ] Run isolation: different runs don't see each other's keys

---

### Story #171: KVStore Multi-Operation API (3 hours)
**File**: `crates/primitives/src/kv.rs`

**Deliverable**: KVTransaction for atomic multi-key operations

**Implementation**:
```rust
/// Transaction handle for multi-key KV operations
pub struct KVTransaction<'a> {
    txn: &'a mut TransactionContext,
    run_id: RunId,
}

impl<'a> KVTransaction<'a> {
    pub fn get(&mut self, key: &str) -> Result<Option<Value>> {
        let storage_key = Key::new_kv(Namespace::for_run(self.run_id), key);
        self.txn.get(&storage_key)
    }

    pub fn put(&mut self, key: &str, value: Value) -> Result<()> {
        let storage_key = Key::new_kv(Namespace::for_run(self.run_id), key);
        self.txn.put(storage_key, value)
    }

    pub fn delete(&mut self, key: &str) -> Result<()> {
        let storage_key = Key::new_kv(Namespace::for_run(self.run_id), key);
        self.txn.delete(storage_key)
    }
}

impl KVStore {
    /// Execute multiple KV operations atomically
    pub fn transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut KVTransaction<'_>) -> Result<T>,
    {
        self.db.transaction(run_id, |txn| {
            let mut kv_txn = KVTransaction { txn, run_id };
            f(&mut kv_txn)
        })
    }
}
```

**Tests**:
- [ ] Multi-key put is atomic
- [ ] Multi-key delete is atomic
- [ ] Transaction rollback on error
- [ ] Read-your-writes within transaction

---

### Story #172: KVStore List Operations (3 hours)
**File**: `crates/primitives/src/kv.rs`

**Deliverable**: list() and list_with_values() with prefix filtering

**Implementation**:
```rust
impl KVStore {
    /// List keys with optional prefix filter
    pub fn list(&self, run_id: RunId, prefix: Option<&str>) -> Result<Vec<String>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let scan_prefix = match prefix {
                Some(p) => Key::new_kv(ns, p),
                None => Key::new_kv(ns, ""),
            };

            let results = txn.scan_prefix(&scan_prefix)?;

            Ok(results
                .into_iter()
                .map(|(key, _)| key.user_key_string())
                .collect())
        })
    }

    /// List key-value pairs with optional prefix filter
    pub fn list_with_values(
        &self,
        run_id: RunId,
        prefix: Option<&str>,
    ) -> Result<Vec<(String, Value)>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let scan_prefix = match prefix {
                Some(p) => Key::new_kv(ns, p),
                None => Key::new_kv(ns, ""),
            };

            let results = txn.scan_prefix(&scan_prefix)?;

            Ok(results
                .into_iter()
                .map(|(key, value)| (key.user_key_string(), value))
                .collect())
        })
    }
}
```

**Tests**:
- [ ] list() returns all keys
- [ ] list() with prefix filters correctly
- [ ] list_with_values() returns key-value pairs
- [ ] Empty list when no keys match
- [ ] Run isolation in list operations

---

### Story #173: KVStoreExt Transaction Extension (3 hours)
**File**: `crates/primitives/src/extensions.rs`

**Deliverable**: KVStoreExt implementation for cross-primitive transactions

**Implementation**:
```rust
impl KVStoreExt for TransactionContext {
    fn kv_get(&mut self, key: &str) -> Result<Option<Value>> {
        // Delegate to KVStore internal logic
        let storage_key = Key::new_kv(self.namespace(), key);
        self.get(&storage_key)
    }

    fn kv_put(&mut self, key: &str, value: Value) -> Result<()> {
        let storage_key = Key::new_kv(self.namespace(), key);
        self.put(storage_key, value)
    }

    fn kv_delete(&mut self, key: &str) -> Result<()> {
        let storage_key = Key::new_kv(self.namespace(), key);
        self.delete(storage_key)
    }
}
```

**Tests**:
- [ ] kv_get() works in cross-primitive transaction
- [ ] kv_put() works in cross-primitive transaction
- [ ] kv_delete() works in cross-primitive transaction
- [ ] Cross-primitive atomicity (KV + other primitive)

---

## Epic 15: EventLog Primitive (6 stories, 1.5 days)

**Goal**: Immutable append-only event stream with causal hash chaining

**Dependencies**: Epic 13 complete

**Deliverables**:
- EventLog struct
- Event structure with hash chaining
- Append, read, verify operations
- EventLogExt transaction extension

### Story #174: EventLog Core & Event Structure (4 hours) 🔴 FOUNDATION
**File**: `crates/primitives/src/event_log.rs`

**Deliverable**: EventLog struct and Event data structure

**Implementation**:
```rust
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use in_mem_engine::Database;
use in_mem_core::{RunId, Value, Result};

/// An event in the log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Sequence number (auto-assigned, monotonic per run)
    pub sequence: u64,
    /// Event type (user-defined category)
    pub event_type: String,
    /// Event payload (arbitrary data)
    pub payload: Value,
    /// Timestamp when event was appended
    pub timestamp: i64,
    /// Hash of previous event (for chaining)
    pub prev_hash: [u8; 32],
    /// Hash of this event
    pub hash: [u8; 32],
}

/// EventLog metadata stored per run
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventLogMeta {
    next_sequence: u64,
    head_hash: [u8; 32],
}

impl Default for EventLogMeta {
    fn default() -> Self {
        Self {
            next_sequence: 0,
            head_hash: [0u8; 32],  // Genesis hash
        }
    }
}

/// Immutable append-only event stream
///
/// DESIGN: Single-writer-ordered per run.
/// All appends serialize through CAS on metadata key.
pub struct EventLog {
    db: Arc<Database>,
}

impl EventLog {
    /// Create new EventLog instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}
```

**Tests**:
- [ ] Event struct serializes/deserializes correctly
- [ ] EventLogMeta default has genesis hash
- [ ] EventLog::new() creates instance

---

### Story #175: EventLog Append with Hash Chaining (5 hours)
**File**: `crates/primitives/src/event_log.rs`

**Deliverable**: Append operation with automatic sequence and hash chain

**Implementation**:
```rust
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

impl EventLog {
    /// Append a new event to the log
    ///
    /// Returns the assigned sequence number and event hash.
    /// Serializes through CAS on metadata key.
    pub fn append(
        &self,
        run_id: RunId,
        event_type: &str,
        payload: Value,
    ) -> Result<(u64, [u8; 32])> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);

            // Read current metadata (or default)
            let meta_key = Key::new_event_meta(ns.clone());
            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => EventLogMeta::default(),
            };

            // Compute event hash
            let sequence = meta.next_sequence;
            let timestamp = Timestamp::now().0;
            let hash = compute_event_hash(
                sequence,
                event_type,
                &payload,
                timestamp,
                &meta.head_hash,
            );

            // Build event
            let event = Event {
                sequence,
                event_type: event_type.to_string(),
                payload: payload.clone(),
                timestamp,
                prev_hash: meta.head_hash,
                hash,
            };

            // Write event
            let event_key = Key::new_event(ns.clone(), sequence);
            txn.put(event_key, Value::from_json(serde_json::to_value(&event)?)?)?;

            // Update metadata (CAS semantics through transaction)
            let new_meta = EventLogMeta {
                next_sequence: sequence + 1,
                head_hash: hash,
            };
            txn.put(meta_key, Value::from_json(serde_json::to_value(&new_meta)?)?)?;

            Ok((sequence, hash))
        })
    }
}

/// Compute event hash (causal, not cryptographic)
fn compute_event_hash(
    sequence: u64,
    event_type: &str,
    payload: &Value,
    timestamp: i64,
    prev_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = DefaultHasher::new();
    sequence.hash(&mut hasher);
    event_type.hash(&mut hasher);
    // Hash payload as JSON string for determinism
    serde_json::to_string(payload).unwrap_or_default().hash(&mut hasher);
    timestamp.hash(&mut hasher);
    prev_hash.hash(&mut hasher);

    // Convert u64 to [u8; 32] (padded for future SHA-256)
    let h = hasher.finish();
    let mut result = [0u8; 32];
    result[0..8].copy_from_slice(&h.to_le_bytes());
    result
}
```

**Tests**:
- [ ] append() returns sequence 0 for first event
- [ ] append() increments sequence
- [ ] append() chains hashes correctly
- [ ] Parallel appends serialize (no sequence gaps)
- [ ] Hash chain is deterministic (same inputs → same hash)

---

### Story #176: EventLog Read Operations (4 hours)
**File**: `crates/primitives/src/event_log.rs`

**Deliverable**: read, read_range, head, len, iter operations

**Implementation**:
```rust
impl EventLog {
    /// Read a single event by sequence number
    pub fn read(&self, run_id: RunId, sequence: u64) -> Result<Option<Event>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let event_key = Key::new_event(ns, sequence);

            match txn.get(&event_key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }

    /// Read a range of events [start, end)
    pub fn read_range(
        &self,
        run_id: RunId,
        start: u64,
        end: u64,
    ) -> Result<Vec<Event>> {
        self.db.transaction(run_id, |txn| {
            let mut events = Vec::new();
            let ns = Namespace::for_run(run_id);

            for seq in start..end {
                let event_key = Key::new_event(ns.clone(), seq);
                if let Some(v) = txn.get(&event_key)? {
                    let event: Event = serde_json::from_value(v.into_json()?)?;
                    events.push(event);
                }
            }

            Ok(events)
        })
    }

    /// Get the latest event (head of the log)
    pub fn head(&self, run_id: RunId) -> Result<Option<Event>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Ok(None),
            };

            if meta.next_sequence == 0 {
                return Ok(None);
            }

            let event_key = Key::new_event(ns, meta.next_sequence - 1);
            match txn.get(&event_key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }

    /// Get the current length of the log
    pub fn len(&self, run_id: RunId) -> Result<u64> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let meta_key = Key::new_event_meta(ns);

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => EventLogMeta::default(),
            };

            Ok(meta.next_sequence)
        })
    }

    /// Check if log is empty
    pub fn is_empty(&self, run_id: RunId) -> Result<bool> {
        Ok(self.len(run_id)? == 0)
    }
}
```

**Tests**:
- [ ] read() returns event by sequence
- [ ] read() returns None for invalid sequence
- [ ] read_range() returns events in order
- [ ] head() returns latest event
- [ ] head() returns None for empty log
- [ ] len() returns correct count
- [ ] is_empty() works correctly

---

### Story #177: EventLog Chain Verification (4 hours)
**File**: `crates/primitives/src/event_log.rs`

**Deliverable**: verify_chain() validates hash chain integrity

**Implementation**:
```rust
/// Chain verification result
#[derive(Debug)]
pub struct ChainVerification {
    pub is_valid: bool,
    pub length: u64,
    pub first_invalid: Option<u64>,
    pub error: Option<String>,
}

impl EventLog {
    /// Verify chain integrity from start to end
    pub fn verify_chain(&self, run_id: RunId) -> Result<ChainVerification> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Ok(ChainVerification {
                    is_valid: true,
                    length: 0,
                    first_invalid: None,
                    error: None,
                }),
            };

            let mut prev_hash = [0u8; 32];  // Genesis

            for seq in 0..meta.next_sequence {
                let event_key = Key::new_event(ns.clone(), seq);
                let event: Event = match txn.get(&event_key)? {
                    Some(v) => serde_json::from_value(v.into_json()?)?,
                    None => return Ok(ChainVerification {
                        is_valid: false,
                        length: meta.next_sequence,
                        first_invalid: Some(seq),
                        error: Some(format!("Missing event at sequence {}", seq)),
                    }),
                };

                // Verify prev_hash
                if event.prev_hash != prev_hash {
                    return Ok(ChainVerification {
                        is_valid: false,
                        length: meta.next_sequence,
                        first_invalid: Some(seq),
                        error: Some(format!("prev_hash mismatch at sequence {}", seq)),
                    });
                }

                // Verify computed hash
                let computed = compute_event_hash(
                    event.sequence,
                    &event.event_type,
                    &event.payload,
                    event.timestamp,
                    &event.prev_hash,
                );

                if computed != event.hash {
                    return Ok(ChainVerification {
                        is_valid: false,
                        length: meta.next_sequence,
                        first_invalid: Some(seq),
                        error: Some(format!("Hash mismatch at sequence {}", seq)),
                    });
                }

                prev_hash = event.hash;
            }

            Ok(ChainVerification {
                is_valid: true,
                length: meta.next_sequence,
                first_invalid: None,
                error: None,
            })
        })
    }
}
```

**Tests**:
- [ ] verify_chain() returns valid for correct chain
- [ ] verify_chain() detects missing event
- [ ] verify_chain() detects prev_hash mismatch
- [ ] verify_chain() detects hash corruption
- [ ] Empty log verifies as valid

---

### Story #178: EventLog Query by Type (3 hours)
**File**: `crates/primitives/src/event_log.rs`

**Deliverable**: read_by_type() filters events by event_type

**Implementation**:
```rust
impl EventLog {
    /// Read events by type
    ///
    /// Note: Linear scan in M3. Future optimization: add type index.
    pub fn read_by_type(
        &self,
        run_id: RunId,
        event_type: &str,
    ) -> Result<Vec<Event>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Ok(Vec::new()),
            };

            let mut events = Vec::new();

            for seq in 0..meta.next_sequence {
                let event_key = Key::new_event(ns.clone(), seq);
                if let Some(v) = txn.get(&event_key)? {
                    let event: Event = serde_json::from_value(v.into_json()?)?;
                    if event.event_type == event_type {
                        events.push(event);
                    }
                }
            }

            Ok(events)
        })
    }

    /// Iterate over all events in order
    pub fn iter(&self, run_id: RunId) -> Result<impl Iterator<Item = Event>> {
        let events = self.read_range(run_id, 0, self.len(run_id)?)?;
        Ok(events.into_iter())
    }
}
```

**Tests**:
- [ ] read_by_type() returns matching events
- [ ] read_by_type() returns empty for no matches
- [ ] iter() returns events in sequence order

---

### Story #179: EventLogExt Transaction Extension (3 hours)
**File**: `crates/primitives/src/extensions.rs`

**Deliverable**: EventLogExt implementation and append-only enforcement

**Implementation**:
```rust
impl EventLogExt for TransactionContext {
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64> {
        // Implementation mirrors EventLog::append() internals
        // ... (delegate to shared internal function)
    }

    fn event_read(&mut self, sequence: u64) -> Result<Option<Event>> {
        // ... (delegate to shared internal function)
    }
}

impl EventLog {
    /// Update is NOT allowed - EventLog is append-only
    pub fn update(&self, _run_id: RunId, _sequence: u64, _payload: Value) -> Result<()> {
        Err(Error::InvalidOperation(
            "EventLog is append-only. Use append() to add new events.".to_string()
        ))
    }

    /// Delete is NOT allowed - EventLog is immutable
    pub fn delete(&self, _run_id: RunId, _sequence: u64) -> Result<()> {
        Err(Error::InvalidOperation(
            "EventLog is immutable. Events cannot be deleted.".to_string()
        ))
    }
}
```

**Tests**:
- [ ] event_append() works in cross-primitive transaction
- [ ] event_read() works in cross-primitive transaction
- [ ] update() returns error
- [ ] delete() returns error
- [ ] Append-only invariant enforced

---

## Epic 16: StateCell Primitive (5 stories, 1 day)

**Goal**: CAS-based versioned cells for coordination

**Dependencies**: Epic 13 complete

**Deliverables**:
- StateCell struct
- State structure with version
- init, read, cas, set, delete, transition operations
- StateCellExt transaction extension

### Story #180: StateCell Core & State Structure (3 hours) 🔴 FOUNDATION
**File**: `crates/primitives/src/state_cell.rs`

**Deliverable**: StateCell struct and State data structure

**Implementation**:
```rust
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use in_mem_engine::Database;
use in_mem_core::{RunId, Value, Result};

/// State cell value with version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    /// Current state value
    pub value: Value,
    /// Version for CAS operations
    pub version: u64,
    /// Last updated timestamp
    pub updated_at: i64,
}

/// CAS-based versioned cells for coordination
///
/// WHY "StateCell" NOT "StateMachine":
/// In M3, this is a versioned CAS cell. It does NOT enforce allowed
/// transitions, guards, or terminal states. A true StateMachine
/// with transition definitions may be added in M5+.
pub struct StateCell {
    db: Arc<Database>,
}

impl StateCell {
    /// Create new StateCell instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}
```

**Tests**:
- [ ] State struct serializes/deserializes
- [ ] StateCell::new() creates instance

---

### Story #181: StateCell Read/Init/Delete Operations (3 hours)
**File**: `crates/primitives/src/state_cell.rs`

**Deliverable**: read, init, delete, exists, list operations

**Implementation**:
```rust
impl StateCell {
    /// Read current state
    pub fn read(&self, run_id: RunId, name: &str) -> Result<Option<State>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let key = Key::new_state(ns, name);

            match txn.get(&key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }

    /// Initialize state cell with initial value
    ///
    /// Fails if cell already exists (use CAS for updates)
    pub fn init(&self, run_id: RunId, name: &str, initial: Value) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let key = Key::new_state(ns, name);

            // Check if already exists
            if txn.get(&key)?.is_some() {
                return Err(Error::AlreadyExists(format!("State cell '{}' already exists", name)));
            }

            let state = State {
                value: initial,
                version: 1,
                updated_at: Timestamp::now().0,
            };

            txn.put(key, Value::from_json(serde_json::to_value(&state)?)?)
        })
    }

    /// Delete state cell
    pub fn delete(&self, run_id: RunId, name: &str) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let key = Key::new_state(ns, name);
            txn.delete(key)
        })
    }

    /// Check if state cell exists
    pub fn exists(&self, run_id: RunId, name: &str) -> Result<bool> {
        Ok(self.read(run_id, name)?.is_some())
    }

    /// List all state cell names
    pub fn list(&self, run_id: RunId) -> Result<Vec<String>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let prefix = Key::new_state(ns, "");

            let results = txn.scan_prefix(&prefix)?;
            Ok(results.into_iter().map(|(k, _)| k.user_key_string()).collect())
        })
    }
}
```

**Tests**:
- [ ] init() creates new cell with version 1
- [ ] init() fails if cell exists
- [ ] read() returns state with version
- [ ] read() returns None if not exists
- [ ] delete() removes cell
- [ ] exists() returns correct boolean
- [ ] list() returns all cell names

---

### Story #182: StateCell CAS & Set Operations (4 hours)
**File**: `crates/primitives/src/state_cell.rs`

**Deliverable**: cas() and set() operations

**Implementation**:
```rust
impl StateCell {
    /// Compare-and-swap state
    ///
    /// Atomically updates state only if current version matches expected.
    /// Returns new version on success.
    pub fn cas(
        &self,
        run_id: RunId,
        name: &str,
        expected_version: u64,
        new_value: Value,
    ) -> Result<u64> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let key = Key::new_state(ns, name);

            // Read current state
            let current: State = match txn.get(&key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Err(Error::NotFound(format!("State cell '{}' not found", name))),
            };

            // Check version
            if current.version != expected_version {
                return Err(Error::CASConflict {
                    key: name.to_string(),
                    expected: expected_version,
                    actual: current.version,
                });
            }

            // Update
            let new_version = current.version + 1;
            let new_state = State {
                value: new_value,
                version: new_version,
                updated_at: Timestamp::now().0,
            };

            txn.put(key, Value::from_json(serde_json::to_value(&new_state)?)?)?;

            Ok(new_version)
        })
    }

    /// Force-set state (unconditional write)
    ///
    /// Use with caution - bypasses version check.
    /// Returns new version.
    pub fn set(&self, run_id: RunId, name: &str, value: Value) -> Result<u64> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let key = Key::new_state(ns, name);

            // Read current state to get version (or start at 1)
            let current_version = match txn.get(&key)? {
                Some(v) => {
                    let state: State = serde_json::from_value(v.into_json()?)?;
                    state.version
                }
                None => 0,
            };

            let new_version = current_version + 1;
            let new_state = State {
                value,
                version: new_version,
                updated_at: Timestamp::now().0,
            };

            txn.put(key, Value::from_json(serde_json::to_value(&new_state)?)?)?;

            Ok(new_version)
        })
    }
}
```

**Tests**:
- [ ] cas() succeeds when version matches
- [ ] cas() fails when version doesn't match
- [ ] cas() increments version
- [ ] cas() fails on non-existent cell
- [ ] set() always succeeds
- [ ] set() increments version
- [ ] set() creates cell if not exists

---

### Story #183: StateCell Transition Closure Pattern (4 hours)
**File**: `crates/primitives/src/state_cell.rs`

**Deliverable**: transition() with automatic retry

**Implementation**:
```rust
impl StateCell {
    /// Execute state transition with automatic retry on conflict
    ///
    /// PURITY REQUIREMENT: The closure `f` must be a pure function.
    /// It may be executed multiple times due to OCC retries.
    ///
    /// Requirements for closure:
    /// - Pure function of inputs (result depends only on &State)
    /// - No I/O (no file, network, console operations)
    /// - No external mutation (don't modify outside variables)
    /// - No irreversible effects (no logging, metrics, API calls)
    /// - Idempotent (same input → same output)
    pub fn transition<F, T>(
        &self,
        run_id: RunId,
        name: &str,
        f: F,
    ) -> Result<T>
    where
        F: Fn(&State) -> Result<(Value, T)>,
    {
        let max_retries = 10;
        let mut attempt = 0;

        loop {
            attempt += 1;

            // Read current state
            let state = self.read(run_id, name)?.ok_or_else(|| {
                Error::NotFound(format!("State cell '{}' not found", name))
            })?;

            // Compute new value (closure may be called multiple times!)
            let (new_value, result) = f(&state)?;

            // Try CAS
            match self.cas(run_id, name, state.version, new_value) {
                Ok(_) => return Ok(result),
                Err(Error::CASConflict { .. }) if attempt < max_retries => {
                    // Retry on conflict
                    std::thread::sleep(Duration::from_micros(100 * attempt as u64));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
}
```

**Tests**:
- [ ] transition() succeeds on first try (no conflict)
- [ ] transition() retries on conflict
- [ ] transition() closure called multiple times on retry
- [ ] transition() returns closure result
- [ ] transition() fails after max retries

---

### Story #184: StateCellExt Transaction Extension (3 hours)
**File**: `crates/primitives/src/extensions.rs`

**Deliverable**: StateCellExt implementation

**Implementation**:
```rust
impl StateCellExt for TransactionContext {
    fn state_read(&mut self, name: &str) -> Result<Option<State>> {
        let key = Key::new_state(self.namespace(), name);
        match self.get(&key)? {
            Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
            None => Ok(None),
        }
    }

    fn state_cas(
        &mut self,
        name: &str,
        expected_version: u64,
        new_value: Value,
    ) -> Result<u64> {
        let key = Key::new_state(self.namespace(), name);

        // Read current state
        let current: State = match self.get(&key)? {
            Some(v) => serde_json::from_value(v.into_json()?)?,
            None => return Err(Error::NotFound(format!("State cell '{}' not found", name))),
        };

        // Check version
        if current.version != expected_version {
            return Err(Error::CASConflict {
                key: name.to_string(),
                expected: expected_version,
                actual: current.version,
            });
        }

        // Update
        let new_version = current.version + 1;
        let new_state = State {
            value: new_value,
            version: new_version,
            updated_at: Timestamp::now().0,
        };

        self.put(key, Value::from_json(serde_json::to_value(&new_state)?)?)?;

        Ok(new_version)
    }

    fn state_set(&mut self, name: &str, value: Value) -> Result<u64> {
        let key = Key::new_state(self.namespace(), name);

        let current_version = match self.get(&key)? {
            Some(v) => {
                let state: State = serde_json::from_value(v.into_json()?)?;
                state.version
            }
            None => 0,
        };

        let new_version = current_version + 1;
        let new_state = State {
            value,
            version: new_version,
            updated_at: Timestamp::now().0,
        };

        self.put(key, Value::from_json(serde_json::to_value(&new_state)?)?)?;

        Ok(new_version)
    }
}
```

**Tests**:
- [ ] state_read() works in transaction
- [ ] state_cas() works in transaction
- [ ] state_set() works in transaction
- [ ] Cross-primitive atomicity (StateCell + KV)

---

## Epic 17: TraceStore Primitive (6 stories, 1.5 days)

**Goal**: Structured reasoning traces with indexing

**Dependencies**: Epic 13 complete

**Deliverables**:
- TraceStore struct
- Trace and TraceType structures
- Record, query, tree operations
- Secondary indices
- TraceStoreExt transaction extension

### Story #185: TraceStore Core & TraceType Structures (4 hours) 🔴 FOUNDATION
**File**: `crates/primitives/src/trace.rs`

**Deliverable**: TraceStore struct and Trace/TraceType data structures

**Implementation**:
```rust
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use in_mem_engine::Database;
use in_mem_core::{RunId, Value, Result};

/// Trace entry type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TraceType {
    /// Tool invocation
    ToolCall {
        tool_name: String,
        arguments: Value,
        result: Option<Value>,
        duration_ms: Option<u64>,
    },
    /// Decision point
    Decision {
        question: String,
        options: Vec<String>,
        chosen: String,
        reasoning: Option<String>,
    },
    /// Query/search
    Query {
        query_type: String,
        query: String,
        results_count: usize,
    },
    /// Thought/reasoning step
    Thought {
        content: String,
        confidence: Option<f64>,
    },
    /// Error/exception
    Error {
        error_type: String,
        message: String,
        recoverable: bool,
    },
    /// Custom trace type
    Custom {
        trace_type: String,
        data: Value,
    },
}

impl TraceType {
    /// Get type name for indexing
    pub fn type_name(&self) -> &str {
        match self {
            TraceType::ToolCall { .. } => "ToolCall",
            TraceType::Decision { .. } => "Decision",
            TraceType::Query { .. } => "Query",
            TraceType::Thought { .. } => "Thought",
            TraceType::Error { .. } => "Error",
            TraceType::Custom { trace_type, .. } => trace_type,
        }
    }
}

/// A trace entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    /// Unique trace ID
    pub id: String,
    /// Parent trace ID (for nested traces)
    pub parent_id: Option<String>,
    /// Trace type and data
    pub trace_type: TraceType,
    /// Timestamp
    pub timestamp: i64,
    /// Optional tags for filtering
    pub tags: Vec<String>,
    /// Optional metadata
    pub metadata: Option<Value>,
}

/// Structured trace storage
///
/// PERFORMANCE WARNING: Optimized for debuggability, not throughput.
/// 3-4 secondary index entries per trace create write amplification.
/// Designed for tens-hundreds of traces per run, NOT telemetry.
pub struct TraceStore {
    db: Arc<Database>,
}

impl TraceStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}
```

**Tests**:
- [ ] TraceType variants serialize/deserialize
- [ ] Trace struct serializes/deserializes
- [ ] type_name() returns correct names

---

### Story #186: TraceStore Record Operations (4 hours)
**File**: `crates/primitives/src/trace.rs`

**Deliverable**: record, record_child, record_with_options operations

**Implementation**:
```rust
/// Options for recording traces
#[derive(Default)]
pub struct TraceOptions {
    pub id: Option<String>,
    pub parent_id: Option<String>,
    pub tags: Vec<String>,
    pub metadata: Option<Value>,
}

impl TraceStore {
    /// Record a new trace
    pub fn record(&self, run_id: RunId, trace_type: TraceType) -> Result<String> {
        self.record_with_options(run_id, trace_type, TraceOptions::default())
    }

    /// Record a trace with parent (for nesting)
    pub fn record_child(
        &self,
        run_id: RunId,
        parent_id: &str,
        trace_type: TraceType,
    ) -> Result<String> {
        self.record_with_options(run_id, trace_type, TraceOptions {
            parent_id: Some(parent_id.to_string()),
            ..Default::default()
        })
    }

    /// Record a trace with custom options
    pub fn record_with_options(
        &self,
        run_id: RunId,
        trace_type: TraceType,
        options: TraceOptions,
    ) -> Result<String> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);

            // Generate or use provided ID
            let trace_id = options.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            // Validate parent exists if specified
            if let Some(ref parent_id) = options.parent_id {
                let parent_key = Key::new_trace(ns.clone(), parent_id);
                if txn.get(&parent_key)?.is_none() {
                    return Err(Error::NotFound(format!("Parent trace '{}' not found", parent_id)));
                }
            }

            let timestamp = Timestamp::now().0;

            // Build trace
            let trace = Trace {
                id: trace_id.clone(),
                parent_id: options.parent_id.clone(),
                trace_type: trace_type.clone(),
                timestamp,
                tags: options.tags.clone(),
                metadata: options.metadata,
            };

            // Write trace
            let trace_key = Key::new_trace(ns.clone(), &trace_id);
            txn.put(trace_key, Value::from_json(serde_json::to_value(&trace)?)?)?;

            // Write indices (Story #187)
            Self::write_indices_internal(txn, &ns, &trace)?;

            Ok(trace_id)
        })
    }
}
```

**Tests**:
- [ ] record() creates trace with generated ID
- [ ] record_child() links to parent
- [ ] record_child() fails if parent doesn't exist
- [ ] record_with_options() uses custom ID
- [ ] Trace has correct timestamp

---

### Story #187: TraceStore Secondary Indices (5 hours)
**File**: `crates/primitives/src/trace.rs`

**Deliverable**: Index management for type, tag, parent, time queries

**Implementation**:
```rust
impl TraceStore {
    /// Write all indices for a trace (internal, called during record)
    fn write_indices_internal(
        txn: &mut TransactionContext,
        ns: &Namespace,
        trace: &Trace,
    ) -> Result<()> {
        // Index by type
        let type_index_key = Key::new_trace_index(
            ns.clone(),
            "type",
            trace.trace_type.type_name(),
            &trace.id,
        );
        txn.put(type_index_key, Value::Null)?;

        // Index by tag (one entry per tag)
        for tag in &trace.tags {
            let tag_index_key = Key::new_trace_index(
                ns.clone(),
                "tag",
                tag,
                &trace.id,
            );
            txn.put(tag_index_key, Value::Null)?;
        }

        // Index by parent (if has parent)
        if let Some(ref parent_id) = trace.parent_id {
            let parent_index_key = Key::new_trace_index(
                ns.clone(),
                "parent",
                parent_id,
                &trace.id,
            );
            txn.put(parent_index_key, Value::Null)?;
        }

        // Index by time (timestamp as big-endian hex)
        let time_key = format!("{:016x}", trace.timestamp);
        let time_index_key = Key::new_trace_index(
            ns.clone(),
            "time",
            &time_key,
            &trace.id,
        );
        txn.put(time_index_key, Value::Null)?;

        Ok(())
    }
}
```

**Tests**:
- [ ] Type index created
- [ ] Tag indices created (multiple tags)
- [ ] Parent index created
- [ ] Time index created
- [ ] All indices atomic with trace write

---

### Story #188: TraceStore Query Operations (4 hours)
**File**: `crates/primitives/src/trace.rs`

**Deliverable**: get, query_by_type, query_by_tag, query_by_time operations

**Implementation**:
```rust
impl TraceStore {
    /// Get a trace by ID
    pub fn get(&self, run_id: RunId, trace_id: &str) -> Result<Option<Trace>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let key = Key::new_trace(ns, trace_id);

            match txn.get(&key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }

    /// Query traces by type
    pub fn query_by_type(
        &self,
        run_id: RunId,
        trace_type_name: &str,
    ) -> Result<Vec<Trace>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let prefix = Key::new_trace_index(ns.clone(), "type", trace_type_name, "");

            let index_entries = txn.scan_prefix(&prefix)?;
            let mut traces = Vec::new();

            for (key, _) in index_entries {
                let trace_id = key.extract_trace_id_from_index();
                let trace_key = Key::new_trace(ns.clone(), &trace_id);
                if let Some(v) = txn.get(&trace_key)? {
                    traces.push(serde_json::from_value(v.into_json()?)?);
                }
            }

            Ok(traces)
        })
    }

    /// Query traces by tag
    pub fn query_by_tag(&self, run_id: RunId, tag: &str) -> Result<Vec<Trace>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let prefix = Key::new_trace_index(ns.clone(), "tag", tag, "");

            let index_entries = txn.scan_prefix(&prefix)?;
            let mut traces = Vec::new();

            for (key, _) in index_entries {
                let trace_id = key.extract_trace_id_from_index();
                let trace_key = Key::new_trace(ns.clone(), &trace_id);
                if let Some(v) = txn.get(&trace_key)? {
                    traces.push(serde_json::from_value(v.into_json()?)?);
                }
            }

            Ok(traces)
        })
    }

    /// Query traces in time range
    pub fn query_by_time(
        &self,
        run_id: RunId,
        start: i64,
        end: i64,
    ) -> Result<Vec<Trace>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let start_key = format!("{:016x}", start);
            let prefix = Key::new_trace_index(ns.clone(), "time", &start_key, "");

            let index_entries = txn.scan_prefix(&prefix)?;
            let mut traces = Vec::new();

            for (key, _) in index_entries {
                let trace_id = key.extract_trace_id_from_index();
                let trace_key = Key::new_trace(ns.clone(), &trace_id);
                if let Some(v) = txn.get(&trace_key)? {
                    let trace: Trace = serde_json::from_value(v.into_json()?)?;
                    if trace.timestamp >= start && trace.timestamp < end {
                        traces.push(trace);
                    }
                }
            }

            Ok(traces)
        })
    }

    /// List all trace IDs
    pub fn list(&self, run_id: RunId) -> Result<Vec<String>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let prefix = Key::new_trace(ns, "");

            let results = txn.scan_prefix(&prefix)?;
            Ok(results.into_iter().map(|(k, _)| k.user_key_string()).collect())
        })
    }

    /// Count traces
    pub fn count(&self, run_id: RunId) -> Result<usize> {
        Ok(self.list(run_id)?.len())
    }
}
```

**Tests**:
- [ ] get() returns trace by ID
- [ ] query_by_type() returns matching traces
- [ ] query_by_tag() returns matching traces
- [ ] query_by_time() returns traces in range
- [ ] list() returns all IDs
- [ ] count() returns correct count

---

### Story #189: TraceStore Tree Reconstruction (4 hours)
**File**: `crates/primitives/src/trace.rs`

**Deliverable**: get_children() and get_tree() for parent-child relationships

**Implementation**:
```rust
/// Hierarchical trace tree
pub struct TraceTree {
    pub root: Trace,
    pub children: Vec<TraceTree>,
}

impl TraceStore {
    /// Get all child traces of a parent
    pub fn get_children(&self, run_id: RunId, parent_id: &str) -> Result<Vec<Trace>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let prefix = Key::new_trace_index(ns.clone(), "parent", parent_id, "");

            let index_entries = txn.scan_prefix(&prefix)?;
            let mut traces = Vec::new();

            for (key, _) in index_entries {
                let trace_id = key.extract_trace_id_from_index();
                let trace_key = Key::new_trace(ns.clone(), &trace_id);
                if let Some(v) = txn.get(&trace_key)? {
                    traces.push(serde_json::from_value(v.into_json()?)?);
                }
            }

            Ok(traces)
        })
    }

    /// Get trace tree (recursive)
    pub fn get_tree(&self, run_id: RunId, root_id: &str) -> Result<TraceTree> {
        let root = self.get(run_id, root_id)?.ok_or_else(|| {
            Error::NotFound(format!("Trace '{}' not found", root_id))
        })?;

        let children = self.get_children(run_id, root_id)?;
        let child_trees: Result<Vec<TraceTree>> = children
            .into_iter()
            .map(|child| self.get_tree(run_id, &child.id))
            .collect();

        Ok(TraceTree {
            root,
            children: child_trees?,
        })
    }
}
```

**Tests**:
- [ ] get_children() returns direct children
- [ ] get_children() returns empty for leaf trace
- [ ] get_tree() builds correct hierarchy
- [ ] get_tree() handles deep nesting

---

### Story #190: TraceStoreExt Transaction Extension (3 hours)
**File**: `crates/primitives/src/extensions.rs`

**Deliverable**: TraceStoreExt implementation

**Implementation**:
```rust
impl TraceStoreExt for TransactionContext {
    fn trace_record(&mut self, trace_type: TraceType) -> Result<String> {
        self.trace_record_with_options(trace_type, TraceOptions::default())
    }

    fn trace_record_child(&mut self, parent_id: &str, trace_type: TraceType) -> Result<String> {
        self.trace_record_with_options(trace_type, TraceOptions {
            parent_id: Some(parent_id.to_string()),
            ..Default::default()
        })
    }

    fn trace_record_with_options(
        &mut self,
        trace_type: TraceType,
        options: TraceOptions,
    ) -> Result<String> {
        let ns = self.namespace();
        let trace_id = options.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Validate parent
        if let Some(ref parent_id) = options.parent_id {
            let parent_key = Key::new_trace(ns.clone(), parent_id);
            if self.get(&parent_key)?.is_none() {
                return Err(Error::NotFound(format!("Parent trace '{}' not found", parent_id)));
            }
        }

        let trace = Trace {
            id: trace_id.clone(),
            parent_id: options.parent_id.clone(),
            trace_type,
            timestamp: Timestamp::now().0,
            tags: options.tags,
            metadata: options.metadata,
        };

        // Write trace
        let trace_key = Key::new_trace(ns.clone(), &trace_id);
        self.put(trace_key, Value::from_json(serde_json::to_value(&trace)?)?)?;

        // Write indices
        TraceStore::write_indices_internal(self, &ns, &trace)?;

        Ok(trace_id)
    }
}
```

**Tests**:
- [ ] trace_record() works in transaction
- [ ] trace_record_child() works in transaction
- [ ] Cross-primitive atomicity (Trace + KV)

---

## Epic 18: RunIndex Primitive (6 stories, 1.5 days)

**Goal**: First-class run lifecycle management

**Dependencies**: Epic 13 complete

**Deliverables**:
- RunIndex struct
- RunMetadata and RunStatus structures
- CRUD and lifecycle operations
- Status transition validation
- Cascading delete

### Story #191: RunIndex Core & RunMetadata Structures (4 hours) 🔴 FOUNDATION
**File**: `crates/primitives/src/run_index.rs`

**Deliverable**: RunIndex struct and metadata structures

**Implementation**:
```rust
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use in_mem_engine::Database;
use in_mem_core::{RunId, Value, Result};

/// Run status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunStatus {
    Active,
    Completed,
    Failed,
    Cancelled,
    Paused,
    Archived,
}

/// Run metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub run_id: RunId,
    pub parent_run: Option<RunId>,
    pub status: RunStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    pub tags: Vec<String>,
    pub metadata: Value,
    pub error: Option<String>,
}

/// Query filter for runs
#[derive(Default)]
pub struct RunQuery {
    pub status: Option<RunStatus>,
    pub tags: Option<Vec<String>>,
    pub created_after: Option<i64>,
    pub created_before: Option<i64>,
    pub parent_run: Option<RunId>,
    pub limit: Option<usize>,
    pub include_archived: bool,
}

/// Options for creating a run
#[derive(Default)]
pub struct CreateRunOptions {
    pub run_id: Option<RunId>,
    pub parent_run: Option<RunId>,
    pub tags: Vec<String>,
    pub metadata: Value,
}

/// First-class run lifecycle management
pub struct RunIndex {
    db: Arc<Database>,
}

impl RunIndex {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}
```

**Tests**:
- [ ] RunStatus variants serialize/deserialize
- [ ] RunMetadata serializes/deserializes
- [ ] RunIndex::new() creates instance

---

### Story #192: RunIndex Create & Get Operations (4 hours)
**File**: `crates/primitives/src/run_index.rs`

**Deliverable**: create_run, create_run_with_options, get_run operations

**Implementation**:
```rust
impl RunIndex {
    /// Create a new run
    pub fn create_run(&self, namespace: &Namespace) -> Result<RunMetadata> {
        self.create_run_with_options(namespace, CreateRunOptions::default())
    }

    /// Create a new run with options
    pub fn create_run_with_options(
        &self,
        namespace: &Namespace,
        options: CreateRunOptions,
    ) -> Result<RunMetadata> {
        let run_id = options.run_id.unwrap_or_else(RunId::new);

        self.db.transaction(run_id, |txn| {
            let ns = namespace.clone();
            let now = Timestamp::now().0;

            let metadata = RunMetadata {
                run_id,
                parent_run: options.parent_run,
                status: RunStatus::Active,
                created_at: now,
                updated_at: now,
                completed_at: None,
                tags: options.tags.clone(),
                metadata: options.metadata.clone(),
                error: None,
            };

            // Write run metadata
            let run_key = Key::new_run(ns.clone(), run_id);
            txn.put(run_key, Value::from_json(serde_json::to_value(&metadata)?)?)?;

            // Write indices
            Self::write_indices_internal(txn, &ns, &metadata)?;

            Ok(metadata)
        })
    }

    /// Get run metadata
    pub fn get_run(&self, run_id: RunId) -> Result<Option<RunMetadata>> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let key = Key::new_run(ns, run_id);

            match txn.get(&key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }

    fn write_indices_internal(
        txn: &mut TransactionContext,
        ns: &Namespace,
        metadata: &RunMetadata,
    ) -> Result<()> {
        // Index by status
        let status_key = Key::new_run_index(
            ns.clone(),
            "status",
            &format!("{:?}", metadata.status),
            metadata.run_id,
        );
        txn.put(status_key, Value::Null)?;

        // Index by tag
        for tag in &metadata.tags {
            let tag_key = Key::new_run_index(ns.clone(), "tag", tag, metadata.run_id);
            txn.put(tag_key, Value::Null)?;
        }

        // Index by parent
        if let Some(parent_id) = metadata.parent_run {
            let parent_key = Key::new_run_index(
                ns.clone(),
                "parent",
                &parent_id.to_string(),
                metadata.run_id,
            );
            txn.put(parent_key, Value::Null)?;
        }

        Ok(())
    }
}
```

**Tests**:
- [ ] create_run() creates Active run
- [ ] create_run_with_options() uses custom run_id
- [ ] create_run_with_options() links to parent
- [ ] get_run() returns metadata
- [ ] get_run() returns None if not found

---

### Story #193: RunIndex Status Update & Transition Validation (5 hours)
**File**: `crates/primitives/src/run_index.rs`

**Deliverable**: Status transition validation and update operations

**Implementation**:
```rust
impl RunIndex {
    /// Check if status transition is valid
    fn is_valid_transition(from: RunStatus, to: RunStatus) -> bool {
        use RunStatus::*;
        match (from, to) {
            // From Active
            (Active, Completed) => true,
            (Active, Failed) => true,
            (Active, Cancelled) => true,
            (Active, Paused) => true,
            (Active, Archived) => true,

            // From Paused
            (Paused, Active) => true,
            (Paused, Cancelled) => true,
            (Paused, Archived) => true,

            // From terminal states
            (Completed, Archived) => true,
            (Failed, Archived) => true,
            (Cancelled, Archived) => true,

            // Everything else is invalid
            _ => false,
        }
    }

    /// Update run status
    pub fn update_status(
        &self,
        run_id: RunId,
        status: RunStatus,
    ) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let key = Key::new_run(ns.clone(), run_id);

            let mut metadata: RunMetadata = match txn.get(&key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Err(Error::NotFound(format!("Run '{}' not found", run_id))),
            };

            // Validate transition
            if !Self::is_valid_transition(metadata.status, status) {
                return Err(Error::InvalidStatusTransition {
                    from: metadata.status,
                    to: status,
                });
            }

            // Remove old status index
            let old_status_key = Key::new_run_index(
                ns.clone(),
                "status",
                &format!("{:?}", metadata.status),
                run_id,
            );
            txn.delete(old_status_key)?;

            // Update metadata
            metadata.status = status;
            metadata.updated_at = Timestamp::now().0;

            if matches!(status, RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled) {
                metadata.completed_at = Some(Timestamp::now().0);
            }

            // Write updated metadata
            txn.put(key, Value::from_json(serde_json::to_value(&metadata)?)?)?;

            // Add new status index
            let new_status_key = Key::new_run_index(
                ns.clone(),
                "status",
                &format!("{:?}", status),
                run_id,
            );
            txn.put(new_status_key, Value::Null)?;

            Ok(())
        })
    }

    /// Complete run successfully
    pub fn complete_run(&self, run_id: RunId) -> Result<()> {
        self.update_status(run_id, RunStatus::Completed)
    }

    /// Fail run with error message
    pub fn fail_run(&self, run_id: RunId, error: &str) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);
            let key = Key::new_run(ns.clone(), run_id);

            let mut metadata: RunMetadata = match txn.get(&key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Err(Error::NotFound(format!("Run '{}' not found", run_id))),
            };

            if !Self::is_valid_transition(metadata.status, RunStatus::Failed) {
                return Err(Error::InvalidStatusTransition {
                    from: metadata.status,
                    to: RunStatus::Failed,
                });
            }

            // Remove old status index
            let old_status_key = Key::new_run_index(
                ns.clone(),
                "status",
                &format!("{:?}", metadata.status),
                run_id,
            );
            txn.delete(old_status_key)?;

            metadata.status = RunStatus::Failed;
            metadata.updated_at = Timestamp::now().0;
            metadata.completed_at = Some(Timestamp::now().0);
            metadata.error = Some(error.to_string());

            txn.put(key, Value::from_json(serde_json::to_value(&metadata)?)?)?;

            let new_status_key = Key::new_run_index(
                ns.clone(),
                "status",
                "Failed",
                run_id,
            );
            txn.put(new_status_key, Value::Null)?;

            Ok(())
        })
    }

    /// Add tags to run
    pub fn add_tags(&self, run_id: RunId, tags: &[String]) -> Result<()> {
        // ... implementation
    }

    /// Update run metadata
    pub fn update_metadata(&self, run_id: RunId, metadata: Value) -> Result<()> {
        // ... implementation
    }
}
```

**Tests**:
- [ ] Valid transitions succeed
- [ ] Invalid transitions fail with error
- [ ] complete_run() sets Completed status
- [ ] fail_run() sets Failed status with error message
- [ ] No resurrection (Completed → Active fails)
- [ ] Archived is terminal

---

### Story #194: RunIndex Query Operations & Indices (4 hours)
**File**: `crates/primitives/src/run_index.rs`

**Deliverable**: query_runs, list_runs, get_child_runs operations

**Implementation**:
```rust
impl RunIndex {
    /// Query runs with filters
    pub fn query_runs(&self, query: RunQuery) -> Result<Vec<RunMetadata>> {
        // Use indices for efficient filtering
        // ... implementation
    }

    /// List all run IDs
    pub fn list_runs(&self) -> Result<Vec<RunId>> {
        // ... implementation
    }

    /// Get child runs (forked from parent)
    pub fn get_child_runs(&self, parent_run: RunId) -> Result<Vec<RunMetadata>> {
        // Use parent index
        // ... implementation
    }

    /// Get run statistics
    pub fn get_stats(&self) -> Result<RunStats> {
        // ... implementation
    }
}

pub struct RunStats {
    pub total_runs: usize,
    pub active_runs: usize,
    pub completed_runs: usize,
    pub failed_runs: usize,
}
```

**Tests**:
- [ ] query_runs() filters by status
- [ ] query_runs() filters by tag
- [ ] query_runs() filters by time range
- [ ] query_runs() excludes archived by default
- [ ] list_runs() returns all IDs
- [ ] get_child_runs() returns forked runs
- [ ] get_stats() returns correct counts

---

### Story #195: RunIndex Delete & Archive Operations (5 hours)
**File**: `crates/primitives/src/run_index.rs`

**Deliverable**: delete_run (cascading) and archive_run (soft delete)

**Implementation**:
```rust
impl RunIndex {
    /// Delete run and all associated data (CASCADING HARD DELETE)
    ///
    /// This is IRREVERSIBLE. Use archive_run() for soft delete.
    pub fn delete_run(&self, run_id: RunId) -> Result<()> {
        self.db.transaction(run_id, |txn| {
            let ns = Namespace::for_run(run_id);

            // Get run metadata (to remove indices)
            let run_key = Key::new_run(ns.clone(), run_id);
            let metadata: RunMetadata = match txn.get(&run_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Err(Error::NotFound(format!("Run '{}' not found", run_id))),
            };

            // Delete all KV keys for this run
            let kv_prefix = Key::new_kv(ns.clone(), "");
            let kv_keys = txn.scan_prefix(&kv_prefix)?;
            for (key, _) in kv_keys {
                txn.delete(key)?;
            }

            // Delete all Event keys for this run
            let event_prefix = Key::new(ns.clone(), TypeTag::Event, b"");
            let event_keys = txn.scan_prefix(&event_prefix)?;
            for (key, _) in event_keys {
                txn.delete(key)?;
            }

            // Delete all State keys for this run
            let state_prefix = Key::new_state(ns.clone(), "");
            let state_keys = txn.scan_prefix(&state_prefix)?;
            for (key, _) in state_keys {
                txn.delete(key)?;
            }

            // Delete all Trace keys for this run (including indices)
            let trace_prefix = Key::new_trace(ns.clone(), "");
            let trace_keys = txn.scan_prefix(&trace_prefix)?;
            for (key, _) in trace_keys {
                txn.delete(key)?;
            }

            // Delete run indices
            Self::delete_indices_internal(txn, &ns, &metadata)?;

            // Delete run metadata
            txn.delete(run_key)?;

            Ok(())
        })
    }

    /// Archive run (soft delete)
    pub fn archive_run(&self, run_id: RunId) -> Result<()> {
        self.update_status(run_id, RunStatus::Archived)
    }

    fn delete_indices_internal(
        txn: &mut TransactionContext,
        ns: &Namespace,
        metadata: &RunMetadata,
    ) -> Result<()> {
        // Delete status index
        let status_key = Key::new_run_index(
            ns.clone(),
            "status",
            &format!("{:?}", metadata.status),
            metadata.run_id,
        );
        txn.delete(status_key)?;

        // Delete tag indices
        for tag in &metadata.tags {
            let tag_key = Key::new_run_index(ns.clone(), "tag", tag, metadata.run_id);
            txn.delete(tag_key)?;
        }

        // Delete parent index
        if let Some(parent_id) = metadata.parent_run {
            let parent_key = Key::new_run_index(
                ns.clone(),
                "parent",
                &parent_id.to_string(),
                metadata.run_id,
            );
            txn.delete(parent_key)?;
        }

        Ok(())
    }
}
```

**Tests**:
- [ ] delete_run() removes run metadata
- [ ] delete_run() removes all KV data
- [ ] delete_run() removes all events
- [ ] delete_run() removes all states
- [ ] delete_run() removes all traces
- [ ] delete_run() removes all indices
- [ ] archive_run() sets Archived status
- [ ] Archived run data still accessible

---

### Story #196: RunIndex Integration with Other Primitives (4 hours)
**File**: `crates/primitives/tests/run_index_integration.rs`

**Deliverable**: Integration tests for run lifecycle with all primitives

**Tests**:
```rust
#[test]
fn test_full_run_lifecycle() {
    // Create run
    // Use all primitives (KV, EventLog, StateCell, TraceStore)
    // Complete run
    // Verify all data accessible
    // Archive run
    // Verify data still accessible
}

#[test]
fn test_cascading_delete() {
    // Create run
    // Write data to all primitives
    // delete_run()
    // Verify ALL data removed
}

#[test]
fn test_run_forking() {
    // Create parent run
    // Write some data
    // Create child run (forked)
    // Verify parent data not visible to child
    // Verify get_child_runs() works
}
```

**Acceptance Criteria**:
- [ ] Run lifecycle works end-to-end
- [ ] Cascading delete removes all data
- [ ] Run forking creates independent runs
- [ ] Status transitions enforced across all operations

---

## Epic 19: Integration & Validation (5 stories, 1.5 days)

**Goal**: Cross-primitive transactions and M3 completion validation

**Dependencies**: Epics 14-18 complete

**Deliverables**:
- Cross-primitive transaction tests
- Run isolation verification
- Recovery tests
- Performance benchmarks
- M3 completion report

### Story #197: Cross-Primitive Transaction Tests (5 hours)
**File**: `crates/primitives/tests/cross_primitive_tests.rs`

**Deliverable**: Atomic operations across all primitives

**Tests**:
```rust
#[test]
fn test_kv_event_state_atomic() {
    db.transaction(run_id, |txn| {
        txn.kv_put("task/status", Value::String("running".into()))?;
        txn.event_append("task_started", json!({"task_id": 1}))?;
        txn.state_cas("workflow", 0, Value::String("step1".into()))?;
        txn.trace_record(TraceType::Thought { content: "Starting task".into(), confidence: None })?;
        Ok(())
    })?;

    // Verify all written atomically
}

#[test]
fn test_cross_primitive_rollback() {
    // Setup: create state with version 1
    // Transaction: KV put + StateCell CAS with wrong version
    // Verify: KV not written (rollback)
}

#[test]
fn test_all_extension_traits_compose() {
    db.transaction(run_id, |txn| {
        // Use all 4 extension traits in one transaction
        txn.kv_put(...)?;
        txn.event_append(...)?;
        txn.state_set(...)?;
        txn.trace_record(...)?;
        Ok(())
    })?;
}
```

**Acceptance Criteria**:
- [ ] All 4 primitives work in single transaction
- [ ] Rollback affects all primitives
- [ ] Extension traits compose correctly

---

### Story #198: Run Isolation Integration Tests (4 hours)
**File**: `crates/primitives/tests/run_isolation_tests.rs`

**Deliverable**: Verify isolation across all primitives

**Tests**:
```rust
#[test]
fn test_kv_isolation() {
    let run1 = RunId::new();
    let run2 = RunId::new();

    kv.put(run1, "key", Value::I64(1))?;
    kv.put(run2, "key", Value::I64(2))?;

    assert_eq!(kv.get(run1, "key")?, Some(Value::I64(1)));
    assert_eq!(kv.get(run2, "key")?, Some(Value::I64(2)));
}

#[test]
fn test_event_log_isolation() {
    // Different runs have independent event sequences
}

#[test]
fn test_state_cell_isolation() {
    // Different runs have independent state cells
}

#[test]
fn test_trace_store_isolation() {
    // Different runs have independent traces
}

#[test]
fn test_cross_run_query_isolation() {
    // Queries only return data from specified run
}
```

**Acceptance Criteria**:
- [ ] KV isolation verified
- [ ] EventLog isolation verified
- [ ] StateCell isolation verified
- [ ] TraceStore isolation verified
- [ ] Queries respect run boundaries

---

### Story #199: Primitive Recovery Tests (5 hours)
**File**: `crates/primitives/tests/recovery_tests.rs`

**Deliverable**: Verify primitives survive crash + WAL replay

**Tests**:
```rust
#[test]
fn test_kv_survives_recovery() {
    kv.put(run_id, "key", value)?;

    // Simulate crash + recovery
    drop(db);
    let db = Database::open(path)?;
    let kv = KVStore::new(db.clone());

    assert_eq!(kv.get(run_id, "key")?, Some(value));
}

#[test]
fn test_event_log_chain_survives_recovery() {
    event_log.append(run_id, "event1", payload1)?;
    event_log.append(run_id, "event2", payload2)?;

    // Recover
    drop(db);
    let db = Database::open(path)?;
    let event_log = EventLog::new(db.clone());

    // Verify chain intact
    assert!(event_log.verify_chain(run_id)?.is_valid);
    assert_eq!(event_log.len(run_id)?, 2);
}

#[test]
fn test_state_cell_version_survives_recovery() {
    state_cell.init(run_id, "cell", initial)?;
    state_cell.cas(run_id, "cell", 1, new_value)?;

    // Recover
    // Verify version is correct
}

#[test]
fn test_trace_indices_survive_recovery() {
    trace_store.record(run_id, trace_type)?;

    // Recover
    // Verify indices work (query_by_type returns trace)
}

#[test]
fn test_run_status_survives_recovery() {
    run_index.create_run(ns)?;
    run_index.update_status(run_id, RunStatus::Completed)?;

    // Recover
    // Verify status is Completed
}
```

**Acceptance Criteria**:
- [ ] KV data preserved after recovery
- [ ] EventLog chain valid after recovery
- [ ] StateCell versions correct after recovery
- [ ] TraceStore indices work after recovery
- [ ] RunIndex status preserved after recovery

---

### Story #200: Primitive Performance Benchmarks (4 hours)
**File**: `crates/primitives/benches/primitive_benchmarks.rs`

**Deliverable**: Benchmark all primitive operations

**Benchmarks**:
```rust
fn bench_kv_put(b: &mut Bencher) {
    // Target: >10K ops/sec
}

fn bench_kv_get(b: &mut Bencher) {
    // Target: >20K ops/sec
}

fn bench_event_append(b: &mut Bencher) {
    // Target: >5K ops/sec (includes hash computation)
}

fn bench_state_cas(b: &mut Bencher) {
    // Target: >5K ops/sec
}

fn bench_trace_record(b: &mut Bencher) {
    // Target: >2K ops/sec (includes index writes)
}

fn bench_cross_primitive_transaction(b: &mut Bencher) {
    // KV + Event + State + Trace in one transaction
    // Target: >1K ops/sec
}
```

**Acceptance Criteria**:
- [ ] KV put: >10K ops/sec
- [ ] KV get: >20K ops/sec
- [ ] EventLog append: >5K ops/sec
- [ ] StateCell CAS: >5K ops/sec
- [ ] TraceStore record: >2K ops/sec
- [ ] Cross-primitive txn: >1K ops/sec

---

### Story #201: M3 Completion Validation (3 hours)
**File**: `docs/milestones/M3_COMPLETION_REPORT.md`

**Deliverable**: M3 completion checklist and report

**Must Verify**:
- [ ] All 7 epics complete
- [ ] All 36 stories delivered
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Benchmarks meet targets
- [ ] Documentation complete

**Completion Report Content**:
1. Epic/Story status summary
2. Test coverage report
3. Benchmark results
4. Known limitations
5. Lessons learned
6. M4 preparation notes

---

## Summary: M3 Structure

### Story Count by Epic

| Epic | Stories | Foundation Story |
|------|---------|------------------|
| Epic 13: Foundation | 3 | #166 |
| Epic 14: KVStore | 5 | #169 |
| Epic 15: EventLog | 6 | #174 |
| Epic 16: StateCell | 5 | #180 |
| Epic 17: TraceStore | 6 | #185 |
| Epic 18: RunIndex | 6 | #191 |
| Epic 19: Integration | 5 | None |
| **Total** | **36** | **6** |

### Critical Path

```
Epic 13 (Foundation)
  ↓
Epic 14 (KVStore)    ← Parallel after #166
Epic 15 (EventLog)   ← Parallel after #166
Epic 16 (StateCell)  ← Parallel after #166
Epic 17 (TraceStore) ← Parallel after #166
Epic 18 (RunIndex)   ← Parallel after #166
  ↓
Epic 19 (Integration) ← After all primitives
```

### Timeline (with 5 Claudes)

| Day | Work |
|-----|------|
| Day 1 | Epic 13 (Foundation) |
| Day 2-3 | Epics 14-18 (Primitives) in parallel |
| Day 4 | Epics 14-18 completion |
| Day 5 | Epic 19 (Integration) |

**Estimated Total**: 5 days with 5 parallel Claudes

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| TypeTag collisions | High | Reserve 0x10+ for future |
| Hash chain bugs | High | Comprehensive verify_chain tests |
| Index inconsistency | Medium | Atomic index writes in transaction |
| TraceStore performance | Medium | Document warning, designed for low volume |
| Status transition bugs | Medium | Exhaustive match in is_valid_transition |
| Cascading delete misses keys | High | Integration test with all primitives |

---

## Open Questions

1. **TTL Cleanup**: When should expired KV entries be cleaned up?
   - Decision: Defer to M4 background tasks

2. **Trace Tree Depth Limit**: Should get_tree() have a max depth?
   - Decision: No limit in M3, add if performance issues

3. **Run Deletion Safety**: Should delete_run require confirmation?
   - Decision: API-level, application decides

4. **Shared Namespace API**: Should primitives expose put_with_namespace?
   - Decision: Yes, but document as advanced usage

---

## Next Steps

1. **Review this plan** - User approval required
2. **Create GitHub Issues** - 7 epic issues, 36 story issues
3. **Begin Epic 13** - Primitives Foundation
4. **Parallel work on Epics 14-18** after Epic 13 complete

**Critical**: Story #166 (Crate Setup) blocks all other M3 work.

---

**Document Version**: 1.0
**Created**: 2026-01-14
**Status**: Planning
