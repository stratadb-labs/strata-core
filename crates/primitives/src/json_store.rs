//! JsonStore: JSON document storage primitive
//!
//! ## Design: STATELESS FACADE
//!
//! JsonStore holds ONLY `Arc<Database>`. No internal state, no caches,
//! no maps, no locks. All data lives in ShardedStore via Key::new_json().
//!
//! ## Run Isolation
//!
//! All operations are scoped to a run_id. Keys are prefixed with the
//! run's namespace, ensuring complete isolation between runs.
//!
//! ## Thread Safety
//!
//! JsonStore is `Send + Sync` and can be safely shared across threads.
//! Multiple JsonStore instances on the same Database are safe.
//!
//! ## API
//!
//! - **Single-Operation API**: `get`, `create`, `set`, `delete_at_path`, `destroy`
//!   Each operation runs in its own implicit transaction.
//!
//! - **Fast Path Reads**: `get`, `exists`, `get_doc`
//!   Use SnapshotView directly for read-only access.
//!
//! ## M5 Architectural Rules
//!
//! This implementation follows the six M5 architectural rules:
//! 1. JSON lives in ShardedStore via Key::new_json()
//! 2. JsonStore is stateless (Arc<Database> only)
//! 3. JSON extends TransactionContext (no separate type)
//! 4. Path semantics in API layer (not storage)
//! 5. WAL remains unified (entry types 0x20-0x23)
//! 6. JSON API feels like other primitives

use in_mem_core::types::{JsonDocId, Key, Namespace, RunId};
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
/// - Uses `Arc<Database>` for all operations
/// - Stores documents via `Key::new_json()` in ShardedStore
/// - Uses SnapshotView for fast path reads
/// - Participates in cross-primitive transactions
///
/// # Example
///
/// ```ignore
/// use in_mem_primitives::JsonStore;
/// use in_mem_engine::Database;
/// use in_mem_core::types::RunId;
/// use in_mem_core::json::JsonValue;
///
/// let db = Arc::new(Database::builder().in_memory().open_temp()?);
/// let json = JsonStore::new(db);
/// let run_id = RunId::new();
/// let doc_id = JsonDocId::new();
///
/// // Create and read document
/// json.create(&run_id, &doc_id, JsonValue::object())?;
/// let value = json.get(&run_id, &doc_id, &JsonPath::root())?;
/// ```
#[derive(Clone)]
pub struct JsonStore {
    db: Arc<Database>, // ONLY state: reference to database
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
    #[allow(dead_code)] // Will be used in Story #274+
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(*run_id)
    }

    /// Build key for JSON document
    #[allow(dead_code)] // Will be used in Story #274+
    fn key_for(&self, run_id: &RunId, doc_id: &JsonDocId) -> Key {
        Key::new_json(self.namespace_for_run(run_id), doc_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonstore_is_stateless() {
        // JsonStore should have size of single Arc pointer
        assert_eq!(
            std::mem::size_of::<JsonStore>(),
            std::mem::size_of::<Arc<Database>>()
        );
    }

    #[test]
    fn test_jsonstore_is_clone() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store1 = JsonStore::new(db.clone());
        let store2 = store1.clone();
        assert!(Arc::ptr_eq(store1.database(), store2.database()));
    }

    #[test]
    fn test_jsonstore_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<JsonStore>();
    }

    #[test]
    fn test_key_for_run_isolation() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);

        let run1 = RunId::new();
        let run2 = RunId::new();
        let doc_id = JsonDocId::new();

        let key1 = store.key_for(&run1, &doc_id);
        let key2 = store.key_for(&run2, &doc_id);

        // Keys for different runs should be different even for same doc_id
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_key_for_same_run() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);

        let run_id = RunId::new();
        let doc_id = JsonDocId::new();

        let key1 = store.key_for(&run_id, &doc_id);
        let key2 = store.key_for(&run_id, &doc_id);

        // Same run and doc_id should produce same key
        assert_eq!(key1, key2);
    }
}
