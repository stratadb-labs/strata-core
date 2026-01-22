# Epic 62: Transaction Unification - Implementation Prompts

**Epic Goal**: Unified TransactionOps trait covering all primitives

**GitHub Issue**: [#466](https://github.com/anibjoshi/in-mem/issues/466)
**Status**: Ready to begin after Epic 60
**Dependencies**: Epic 60 (Core Types)
**Phases**: 2, 3, 4, 5 (incremental)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M9" or "Strata" in the actual codebase or comments.**
>
> - "M9" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Universal entity reference for any in-mem entity`
> **WRONG**: `//! Universal entity reference for any Strata entity`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M9_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M9_ARCHITECTURE.md`
2. **Primitive Contract**: `docs/architecture/PRIMITIVE_CONTRACT.md`
3. **Epic Spec**: `docs/milestones/M9/EPIC_62_TRANSACTION_UNIFICATION.md`
4. **Prompt Header**: `docs/prompts/M9/M9_PROMPT_HEADER.md`

---

## Epic 62 Overview

### CRITICAL: Phased Implementation

> **Do not convert all 7 primitives in one pass.**

This epic spans multiple phases:

| Phase | Stories | Primitives |
|-------|---------|------------|
| Phase 2 | #482, #483, #484 | Trait definition + KV + EventLog |
| Phase 3 | #485 | StateCell + TraceStore |
| Phase 4 | #486 | JsonStore + VectorStore |
| Phase 5 | #487 | RunHandle pattern (finalize) |

### Key Rule: Transaction Trait Covers All Primitives

> Every primitive operation is accessible through the `TransactionOps` trait.

```rust
// CORRECT: All primitives in trait
pub trait TransactionOps {
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError>;
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Version, StrataError>;
    fn state_read(&self, name: &str) -> Result<Option<Versioned<StateValue>>, StrataError>;
    // ... all primitives
}

// WRONG: Missing primitives
pub trait TransactionOps {
    fn kv_get(&self, key: &str) -> Result<Option<Value>>;
    // Missing event, state, trace, json, vector - FORBIDDEN
}
```

### Component Breakdown

| Story | Description | Phase | Priority |
|-------|-------------|-------|----------|
| #482 | TransactionOps Trait Definition | 2 | FOUNDATION |
| #483 | KV Operations in TransactionOps | 2 | CRITICAL |
| #484 | Event Operations in TransactionOps | 2 | CRITICAL |
| #485 | State/Trace Operations in TransactionOps | 3 | CRITICAL |
| #486 | Json/Vector Operations in TransactionOps | 4 | CRITICAL |
| #487 | RunHandle Pattern Implementation | 5 | HIGH |

---

## Phase 2: Trait Definition + KV + EventLog

### Story #482: TransactionOps Trait Definition

**GitHub Issue**: [#482](https://github.com/anibjoshi/in-mem/issues/482)
**Estimated Time**: 3 hours
**Dependencies**: Epic 60 complete
**Phase**: 2
**Blocks**: All other Epic 62 stories

#### Start Story

```bash
gh issue view 482
./scripts/start-story.sh 62 482 transaction-ops-trait
```

#### Implementation

Create `crates/engine/src/transaction_ops.rs`:

```rust
//! TransactionOps trait - unified primitive operations
//!
//! This trait expresses Invariant 3: Everything is Transactional.
//! Every primitive's operations are accessible through this trait,
//! enabling cross-primitive atomic operations.

use crate::{
    EntityRef, Versioned, Version, Timestamp, RunId, StrataError,
    Value, Event, StateValue, Trace, TraceId, TraceType,
    JsonValue, JsonPath, JsonDocId,
    VectorEntry, VectorMatch, VectorId, MetadataFilter,
};

/// Operations available within a transaction
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
///     txn.state_set("last_event", StateValue::from(event_version.as_u64()))?;
///
///     Ok(())
/// })?;
/// ```
pub trait TransactionOps {
    // =========================================================================
    // KV Operations
    // =========================================================================

    /// Get a KV value
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError>;

    /// Put a KV value
    fn kv_put(&mut self, key: &str, value: Value) -> Result<Version, StrataError>;

    /// Delete a KV key
    fn kv_delete(&mut self, key: &str) -> Result<bool, StrataError>;

    /// Check if a KV key exists
    fn kv_exists(&self, key: &str) -> Result<bool, StrataError>;

    // =========================================================================
    // Event Operations
    // =========================================================================

    /// Append an event
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Version, StrataError>;

    /// Read an event by sequence
    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>, StrataError>;

    /// Read a range of events
    fn event_range(&self, start: u64, end: u64) -> Result<Vec<Versioned<Event>>, StrataError>;

    // =========================================================================
    // State Operations
    // =========================================================================

    /// Read a state cell
    fn state_read(&self, name: &str) -> Result<Option<Versioned<StateValue>>, StrataError>;

    /// Set a state cell
    fn state_set(&mut self, name: &str, value: StateValue) -> Result<Version, StrataError>;

    /// Compare-and-swap a state cell
    fn state_cas(&mut self, name: &str, expected: u64, value: StateValue) -> Result<Version, StrataError>;

    /// Delete a state cell
    fn state_delete(&mut self, name: &str) -> Result<bool, StrataError>;

    /// Check if a state cell exists
    fn state_exists(&self, name: &str) -> Result<bool, StrataError>;

    // =========================================================================
    // Trace Operations
    // =========================================================================

    /// Record a trace
    fn trace_record(&mut self, trace_type: TraceType, data: Value) -> Result<Versioned<TraceId>, StrataError>;

    /// Read a trace
    fn trace_read(&self, trace_id: &TraceId) -> Result<Option<Versioned<Trace>>, StrataError>;

    // =========================================================================
    // JSON Operations
    // =========================================================================

    /// Create a JSON document
    fn json_create(&mut self, doc_id: &JsonDocId, value: JsonValue) -> Result<Version, StrataError>;

    /// Get a JSON document
    fn json_get(&self, doc_id: &JsonDocId) -> Result<Option<Versioned<JsonValue>>, StrataError>;

    /// Get a value at a path within a JSON document
    fn json_get_path(&self, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<Versioned<JsonValue>>, StrataError>;

    /// Set a JSON document
    fn json_set(&mut self, doc_id: &JsonDocId, value: JsonValue) -> Result<Version, StrataError>;

    /// Set a value at a path within a JSON document
    fn json_set_path(&mut self, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<Version, StrataError>;

    /// Delete a JSON document
    fn json_delete(&mut self, doc_id: &JsonDocId) -> Result<bool, StrataError>;

    /// Check if a JSON document exists
    fn json_exists(&self, doc_id: &JsonDocId) -> Result<bool, StrataError>;

    // =========================================================================
    // Vector Operations
    // =========================================================================

    /// Upsert a vector
    fn vector_upsert(
        &mut self,
        collection: &str,
        id: VectorId,
        embedding: Vec<f32>,
        metadata: Option<Value>,
    ) -> Result<Version, StrataError>;

    /// Get a vector
    fn vector_get(&self, collection: &str, id: VectorId) -> Result<Option<Versioned<VectorEntry>>, StrataError>;

    /// Delete a vector
    fn vector_delete(&mut self, collection: &str, id: VectorId) -> Result<bool, StrataError>;

    /// Search for similar vectors
    fn vector_search(
        &self,
        collection: &str,
        query: Vec<f32>,
        k: usize,
        filter: Option<MetadataFilter>,
    ) -> Result<Vec<Versioned<VectorMatch>>, StrataError>;
}
```

#### Update lib.rs

```rust
pub mod transaction_ops;
pub use transaction_ops::TransactionOps;
```

#### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Verify trait is object-safe (can be used as dyn TransactionOps)
    fn _assert_object_safe(_: &dyn TransactionOps) {}

    #[test]
    fn test_trait_methods_exist() {
        // This test just verifies the trait compiles with all methods
        // Actual implementation tests are in the Transaction impl
    }
}
```

#### Complete Story

```bash
./scripts/complete-story.sh 482
```

---

### Story #483: KV Operations in TransactionOps

**GitHub Issue**: [#483](https://github.com/anibjoshi/in-mem/issues/483)
**Estimated Time**: 2 hours
**Dependencies**: Story #482
**Phase**: 2

#### Start Story

```bash
gh issue view 483
./scripts/start-story.sh 62 483 kv-transaction-ops
```

#### Implementation

Add to `crates/engine/src/transaction.rs`:

```rust
impl TransactionOps for Transaction {
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError> {
        self.kv_store
            .get(&self.run_id, key)
            .map_err(StrataError::from)
    }

    fn kv_put(&mut self, key: &str, value: Value) -> Result<Version, StrataError> {
        self.kv_store
            .put(&self.run_id, key, value)
            .map_err(StrataError::from)
    }

    fn kv_delete(&mut self, key: &str) -> Result<bool, StrataError> {
        self.kv_store
            .delete(&self.run_id, key)
            .map_err(StrataError::from)
    }

    fn kv_exists(&self, key: &str) -> Result<bool, StrataError> {
        self.kv_store
            .exists(&self.run_id, key)
            .map_err(StrataError::from)
    }

    // ... other methods return unimplemented!() for now
}
```

#### Tests

```rust
#[test]
fn test_kv_through_transaction_ops() {
    let db = Database::new_in_memory();
    let run_id = RunId::new("test");

    db.transaction(&run_id, |txn| {
        // Write through trait
        let version = txn.kv_put("key", Value::from("value"))?;
        assert!(version.is_txn_id());

        // Read through trait
        let result = txn.kv_get("key")?;
        assert!(result.is_some());
        let versioned = result.unwrap();
        assert_eq!(versioned.value, Value::from("value"));

        // Exists through trait
        assert!(txn.kv_exists("key")?);
        assert!(!txn.kv_exists("nonexistent")?);

        Ok(())
    }).unwrap();
}
```

#### Complete Story

```bash
./scripts/complete-story.sh 483
```

---

### Story #484: Event Operations in TransactionOps

**GitHub Issue**: [#484](https://github.com/anibjoshi/in-mem/issues/484)
**Estimated Time**: 2 hours
**Dependencies**: Story #482
**Phase**: 2

#### Start Story

```bash
gh issue view 484
./scripts/start-story.sh 62 484 event-transaction-ops
```

#### Implementation

```rust
impl TransactionOps for Transaction {
    // ... KV methods ...

    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Version, StrataError> {
        self.event_log
            .append(&self.run_id, event_type, payload)
            .map_err(StrataError::from)
    }

    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>, StrataError> {
        self.event_log
            .read(&self.run_id, sequence)
            .map_err(StrataError::from)
    }

    fn event_range(&self, start: u64, end: u64) -> Result<Vec<Versioned<Event>>, StrataError> {
        self.event_log
            .range(&self.run_id, start, end)
            .map_err(StrataError::from)
    }
}
```

#### Tests

```rust
#[test]
fn test_event_through_transaction_ops() {
    let db = Database::new_in_memory();
    let run_id = RunId::new("test");

    db.transaction(&run_id, |txn| {
        // Append through trait
        let version = txn.event_append("user_action", json!({"action": "click"}))?;
        assert!(version.is_sequence());
        assert_eq!(version.as_u64(), 0);

        // Read through trait
        let result = txn.event_read(0)?;
        assert!(result.is_some());
        let versioned = result.unwrap();
        assert_eq!(versioned.value.event_type, "user_action");

        Ok(())
    }).unwrap();
}

#[test]
fn test_kv_and_event_in_same_transaction() {
    let db = Database::new_in_memory();
    let run_id = RunId::new("test");

    db.transaction(&run_id, |txn| {
        // KV operation
        txn.kv_put("config", Value::from("active"))?;

        // Event operation
        let event_version = txn.event_append("config_change", json!({"key": "config"}))?;

        // Both should succeed
        assert!(txn.kv_exists("config")?);
        assert!(txn.event_read(event_version.as_u64())?.is_some());

        Ok(())
    }).unwrap();
}
```

#### Complete Story

```bash
./scripts/complete-story.sh 484
```

---

## Phase 3: State + Trace

### Story #485: State/Trace Operations in TransactionOps

**GitHub Issue**: [#485](https://github.com/anibjoshi/in-mem/issues/485)
**Estimated Time**: 3 hours
**Phase**: 3

#### Start Story

```bash
gh issue view 485
./scripts/start-story.sh 62 485 state-trace-transaction-ops
```

#### Implementation

```rust
impl TransactionOps for Transaction {
    // ... KV and Event methods ...

    // State Operations
    fn state_read(&self, name: &str) -> Result<Option<Versioned<StateValue>>, StrataError> {
        self.state_cell
            .read(&self.run_id, name)
            .map_err(StrataError::from)
    }

    fn state_set(&mut self, name: &str, value: StateValue) -> Result<Version, StrataError> {
        self.state_cell
            .set(&self.run_id, name, value)
            .map_err(StrataError::from)
    }

    fn state_cas(&mut self, name: &str, expected: u64, value: StateValue) -> Result<Version, StrataError> {
        self.state_cell
            .cas(&self.run_id, name, expected, value)
            .map_err(StrataError::from)
    }

    fn state_delete(&mut self, name: &str) -> Result<bool, StrataError> {
        self.state_cell
            .delete(&self.run_id, name)
            .map_err(StrataError::from)
    }

    fn state_exists(&self, name: &str) -> Result<bool, StrataError> {
        self.state_cell
            .exists(&self.run_id, name)
            .map_err(StrataError::from)
    }

    // Trace Operations
    fn trace_record(&mut self, trace_type: TraceType, data: Value) -> Result<Versioned<TraceId>, StrataError> {
        self.trace_store
            .record(&self.run_id, trace_type, data)
            .map_err(StrataError::from)
    }

    fn trace_read(&self, trace_id: &TraceId) -> Result<Option<Versioned<Trace>>, StrataError> {
        self.trace_store
            .read(&self.run_id, trace_id)
            .map_err(StrataError::from)
    }
}
```

#### Complete Story

```bash
./scripts/complete-story.sh 485
```

---

## Phase 4: Json + Vector

### Story #486: Json/Vector Operations in TransactionOps

**GitHub Issue**: [#486](https://github.com/anibjoshi/in-mem/issues/486)
**Estimated Time**: 3 hours
**Phase**: 4

#### Start Story

```bash
gh issue view 486
./scripts/start-story.sh 62 486 json-vector-transaction-ops
```

#### Implementation

Add JSON and Vector operations following the same pattern.

#### Complete Story

```bash
./scripts/complete-story.sh 486
```

---

## Phase 5: RunHandle

### Story #487: RunHandle Pattern Implementation

**GitHub Issue**: [#487](https://github.com/anibjoshi/in-mem/issues/487)
**Estimated Time**: 3 hours
**Phase**: 5

#### Start Story

```bash
gh issue view 487
./scripts/start-story.sh 62 487 run-handle
```

#### Implementation

Create `crates/engine/src/run_handle.rs`:

```rust
//! RunHandle - ergonomic run-scoped API

use crate::{Database, RunId, TransactionOps, StrataError, Versioned, Version, Value};

/// A handle to a specific run
///
/// RunHandle provides ergonomic access to a run's primitives.
/// It enforces run scope (Invariant 5) at the type level.
pub struct RunHandle<'db> {
    run_id: RunId,
    db: &'db Database,
}

impl<'db> RunHandle<'db> {
    /// Create a handle for a run
    pub(crate) fn new(db: &'db Database, run_id: RunId) -> Self {
        Self { run_id, db }
    }

    /// Get the run ID
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// Execute a transaction on this run
    pub fn transaction<F, T>(&self, f: F) -> Result<T, StrataError>
    where
        F: FnOnce(&mut dyn TransactionOps) -> Result<T, StrataError>,
    {
        self.db.transaction(&self.run_id, f)
    }

    // =========================================================================
    // Convenience Methods (non-transactional)
    // =========================================================================

    /// Get a KV value
    pub fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError> {
        self.db.kv().get(&self.run_id, key).map_err(StrataError::from)
    }

    /// Put a KV value
    pub fn kv_put(&self, key: &str, value: Value) -> Result<Version, StrataError> {
        self.db.kv().put(&self.run_id, key, value).map_err(StrataError::from)
    }

    /// Append an event
    pub fn event_append(&self, event_type: &str, payload: Value) -> Result<Version, StrataError> {
        self.db.events().append(&self.run_id, event_type, payload).map_err(StrataError::from)
    }

    // ... similar convenience methods for other primitives
}

// Database extension
impl Database {
    /// Get a handle to a run
    pub fn run(&self, run_id: impl Into<RunId>) -> RunHandle<'_> {
        RunHandle::new(self, run_id.into())
    }
}
```

#### Usage Example

```rust
let db = Database::new_in_memory();

// Get a run handle
let run = db.run("my-run");

// Single operations (non-transactional)
run.kv_put("key", Value::from("value"))?;
let value = run.kv_get("key")?;

// Transaction (atomic)
run.transaction(|txn| {
    txn.kv_put("a", Value::from(1))?;
    txn.event_append("update", json!({"key": "a"}))?;
    Ok(())
})?;
```

#### Tests

```rust
#[test]
fn test_run_handle_single_operations() {
    let db = Database::new_in_memory();
    let run = db.run("test");

    run.kv_put("key", Value::from("value")).unwrap();
    let result = run.kv_get("key").unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap().value, Value::from("value"));
}

#[test]
fn test_run_handle_transaction() {
    let db = Database::new_in_memory();
    let run = db.run("test");

    run.transaction(|txn| {
        txn.kv_put("key", Value::from("value"))?;
        assert!(txn.kv_exists("key")?);
        Ok(())
    }).unwrap();

    // Verify committed
    assert!(run.kv_get("key").unwrap().is_some());
}
```

#### Complete Story

```bash
./scripts/complete-story.sh 487
```

---

## Epic 62 Completion Checklist

### After All Phases Complete

```bash
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
```

### Verify Deliverables

- [ ] TransactionOps trait with all primitive operations
- [ ] All reads are `&self`, all writes are `&mut self`
- [ ] All methods return `Result<T, StrataError>`
- [ ] Transaction impl for all primitives
- [ ] RunHandle pattern implemented
- [ ] Cross-primitive transactions work

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-62-transaction-unification -m "Epic 62: Transaction Unification complete

Delivered:
- TransactionOps trait with all primitive operations
- Transaction impl for KV, Event, State, Trace, Json, Vector
- RunHandle pattern for ergonomic run-scoped access
- Cross-primitive atomic transactions

Stories: #482, #483, #484, #485, #486, #487
"
git push origin develop
gh issue close 466 --comment "Epic 62: Transaction Unification - COMPLETE"
```
