//! RunHandle: Scoped access to primitives within a run
//!
//! ## Design
//!
//! `RunHandle` binds a `RunId` to a `Database`, eliminating the need to
//! pass `run_id` to every operation. It provides:
//!
//! - `kv()`, `events()`, `state()`, `json()`, `vectors()` - primitive handles
//! - `transaction()` - execute atomic cross-primitive transactions
//!
//! ## Usage
//!
//! ```rust,ignore
//! let run = db.run("my-run");
//!
//! // Access primitives directly
//! let value = run.kv().get("key")?;
//! run.events().append("event", json!({}))?;
//!
//! // Or use transactions for atomicity
//! run.transaction(|txn| {
//!     txn.kv_put("key", value)?;
//!     txn.event_append("event", json!({}))?;
//!     Ok(())
//! })?;
//! ```
//!
//! ## Story #478: RunHandle Pattern Implementation

use crate::extensions::{
    EventLogExt, JsonStoreExt, KVStoreExt, StateCellExt, VectorStoreExt,
};
use strata_concurrency::TransactionContext;
use strata_core::contract::{Timestamp, Version, Versioned};
use strata_core::error::Result;
use strata_core::json::{JsonPath, JsonValue};
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use std::sync::Arc;

// ============================================================================
// RunHandle
// ============================================================================

/// Handle to a specific run
///
/// Provides scoped access to all primitives within a run.
/// The run_id is bound to this handle, so operations don't need
/// to specify it repeatedly.
///
/// ## Thread Safety
///
/// `RunHandle` is `Clone`, `Send`, and `Sync`. Multiple threads can
/// share the same `RunHandle` and operate on the same run concurrently.
/// Transaction isolation ensures correctness.
///
/// ## Example
///
/// ```rust,ignore
/// let run = db.run(run_id);
///
/// // Access primitives
/// let value = run.kv().get("key")?;
/// run.events().append("my-event", json!({}))?;
///
/// // Use transactions
/// run.transaction(|txn| {
///     txn.kv_put("key", value)?;
///     txn.event_append("my-event", json!({}))?;
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
    pub fn new(db: Arc<Database>, run_id: RunId) -> Self {
        Self { db, run_id }
    }

    /// Get the run ID
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// Get the underlying database
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    // === Primitive Handles ===

    /// Access the KV primitive for this run
    pub fn kv(&self) -> KvHandle {
        KvHandle::new(self.db.clone(), self.run_id)
    }

    /// Access the Event primitive for this run
    pub fn events(&self) -> EventHandle {
        EventHandle::new(self.db.clone(), self.run_id)
    }

    /// Access the State primitive for this run
    pub fn state(&self) -> StateHandle {
        StateHandle::new(self.db.clone(), self.run_id)
    }

    /// Access the Json primitive for this run
    pub fn json(&self) -> JsonHandle {
        JsonHandle::new(self.db.clone(), self.run_id)
    }

    /// Access the Vector primitive for this run
    pub fn vectors(&self) -> VectorHandle {
        VectorHandle::new(self.db.clone(), self.run_id)
    }

    // === Transactions ===

    /// Execute a transaction within this run
    ///
    /// All operations in the closure are atomic. Either all succeed,
    /// or none do (rollback on error).
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// run.transaction(|txn| {
    ///     let value = txn.kv_get("counter")?;
    ///     txn.kv_put("counter", Value::from(value.unwrap_or(0) + 1))?;
    ///     txn.event_append("counter_incremented", json!({}))?;
    ///     Ok(())
    /// })?;
    /// ```
    pub fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T>,
    {
        self.db.transaction(self.run_id, f)
    }
}

// ============================================================================
// KvHandle
// ============================================================================

/// Handle for KV operations scoped to a run
///
/// Each operation runs in its own implicit transaction.
#[derive(Clone)]
pub struct KvHandle {
    db: Arc<Database>,
    run_id: RunId,
}

impl KvHandle {
    /// Create a new KvHandle
    pub(crate) fn new(db: Arc<Database>, run_id: RunId) -> Self {
        Self { db, run_id }
    }

    /// Get a value by key
    pub fn get(&self, key: &str) -> Result<Option<Versioned<Value>>> {
        self.db.transaction(self.run_id, |txn| {
            let value = txn.kv_get(key)?;
            // Wrap in Versioned - since KVStoreExt returns Option<Value> not Versioned
            Ok(value.map(|v| {
                Versioned::with_timestamp(v, Version::counter(0), Timestamp::now())
            }))
        })
    }

    /// Put a value
    pub fn put(&self, key: &str, value: Value) -> Result<Version> {
        self.db.transaction(self.run_id, |txn| {
            txn.kv_put(key, value)?;
            Ok(Version::counter(1))
        })
    }

    /// Delete a key
    pub fn delete(&self, key: &str) -> Result<bool> {
        self.db.transaction(self.run_id, |txn| {
            txn.kv_delete(key)?;
            Ok(true)
        })
    }

    /// Check if a key exists
    pub fn exists(&self, key: &str) -> Result<bool> {
        self.get(key).map(|v| v.is_some())
    }
}

// ============================================================================
// EventHandle
// ============================================================================

/// Handle for Event operations scoped to a run
#[derive(Clone)]
pub struct EventHandle {
    db: Arc<Database>,
    run_id: RunId,
}

impl EventHandle {
    /// Create a new EventHandle
    pub(crate) fn new(db: Arc<Database>, run_id: RunId) -> Self {
        Self { db, run_id }
    }

    /// Append an event and return sequence number
    pub fn append(&self, event_type: &str, payload: Value) -> Result<u64> {
        self.db.transaction(self.run_id, |txn| {
            txn.event_append(event_type, payload)
        })
    }

    /// Read an event by sequence number
    pub fn read(&self, sequence: u64) -> Result<Option<Value>> {
        self.db.transaction(self.run_id, |txn| {
            txn.event_read(sequence)
        })
    }
}

// ============================================================================
// StateHandle
// ============================================================================

/// Handle for State operations scoped to a run
#[derive(Clone)]
pub struct StateHandle {
    db: Arc<Database>,
    run_id: RunId,
}

impl StateHandle {
    /// Create a new StateHandle
    pub(crate) fn new(db: Arc<Database>, run_id: RunId) -> Self {
        Self { db, run_id }
    }

    /// Read current state
    pub fn read(&self, name: &str) -> Result<Option<Value>> {
        self.db.transaction(self.run_id, |txn| {
            txn.state_read(name)
        })
    }

    /// Compare-and-swap update
    pub fn cas(&self, name: &str, expected_version: u64, new_value: Value) -> Result<u64> {
        self.db.transaction(self.run_id, |txn| {
            txn.state_cas(name, expected_version, new_value)
        })
    }

    /// Unconditional set
    pub fn set(&self, name: &str, value: Value) -> Result<u64> {
        self.db.transaction(self.run_id, |txn| {
            txn.state_set(name, value)
        })
    }
}

// ============================================================================
// JsonHandle
// ============================================================================

/// Handle for JSON operations scoped to a run
#[derive(Clone)]
pub struct JsonHandle {
    db: Arc<Database>,
    run_id: RunId,
}

impl JsonHandle {
    /// Create a new JsonHandle
    pub(crate) fn new(db: Arc<Database>, run_id: RunId) -> Self {
        Self { db, run_id }
    }

    /// Create a new JSON document
    pub fn create(&self, doc_id: &str, value: JsonValue) -> Result<Version> {
        self.db.transaction(self.run_id, |txn| {
            txn.json_create(doc_id, value)
        })
    }

    /// Get value at path in a document
    pub fn get(&self, doc_id: &str, path: &JsonPath) -> Result<Option<JsonValue>> {
        self.db.transaction(self.run_id, |txn| {
            txn.json_get(doc_id, path)
        })
    }

    /// Set value at path in a document
    pub fn set(&self, doc_id: &str, path: &JsonPath, value: JsonValue) -> Result<Version> {
        self.db.transaction(self.run_id, |txn| {
            txn.json_set(doc_id, path, value)
        })
    }
}

// ============================================================================
// VectorHandle
// ============================================================================

/// Handle for Vector operations scoped to a run
///
/// Note: VectorStore operations in transactions are limited.
/// Complex operations like search should use VectorStore directly.
#[derive(Clone)]
pub struct VectorHandle {
    db: Arc<Database>,
    run_id: RunId,
}

impl VectorHandle {
    /// Create a new VectorHandle
    pub(crate) fn new(db: Arc<Database>, run_id: RunId) -> Self {
        Self { db, run_id }
    }

    /// Get a vector by key
    ///
    /// Note: This operation is not supported in cross-primitive transactions.
    /// Use VectorStore::get() directly for vector operations.
    pub fn get(&self, collection: &str, key: &str) -> Result<Option<Vec<f32>>> {
        self.db.transaction(self.run_id, |txn| {
            txn.vector_get(collection, key)
        })
    }

    /// Insert a vector
    ///
    /// Note: This operation is not supported in cross-primitive transactions.
    /// Use VectorStore::insert() directly for vector operations.
    pub fn insert(&self, collection: &str, key: &str, embedding: &[f32]) -> Result<Version> {
        self.db.transaction(self.run_id, |txn| {
            txn.vector_insert(collection, key, embedding)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_handle_is_clone_send_sync() {
        fn assert_clone_send_sync<T: Clone + Send + Sync>() {}
        assert_clone_send_sync::<RunHandle>();
        assert_clone_send_sync::<KvHandle>();
        assert_clone_send_sync::<EventHandle>();
        assert_clone_send_sync::<StateHandle>();
        assert_clone_send_sync::<JsonHandle>();
        assert_clone_send_sync::<VectorHandle>();
    }
}
