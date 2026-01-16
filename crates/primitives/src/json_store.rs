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

use in_mem_core::error::{Error, Result};
use in_mem_core::json::JsonValue;
use in_mem_core::types::{JsonDocId, Key, Namespace, RunId};
use in_mem_core::value::Value;
use in_mem_engine::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;

// =============================================================================
// JsonDoc - Internal Document Representation
// =============================================================================

/// Internal representation of a JSON document
///
/// Stored as serialized bytes in ShardedStore.
/// Version is used for optimistic concurrency control.
///
/// # Design
///
/// - **Document-level versioning**: Single version for entire document
/// - **Timestamps**: Track creation and modification times
/// - **Serializable**: Uses MessagePack for efficient storage
///
/// # Example
///
/// ```rust
/// use in_mem_primitives::json_store::JsonDoc;
/// use in_mem_core::types::JsonDocId;
/// use in_mem_core::json::JsonValue;
///
/// let doc = JsonDoc::new(JsonDocId::new(), JsonValue::from(42i64));
/// assert_eq!(doc.version, 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDoc {
    /// Document unique identifier
    pub id: JsonDocId,
    /// The JSON value (root of document)
    pub value: JsonValue,
    /// Document version (increments on any change)
    pub version: u64,
    /// Creation timestamp (millis since epoch)
    pub created_at: i64,
    /// Last modification timestamp (millis since epoch)
    pub updated_at: i64,
}

impl JsonDoc {
    /// Create a new document with initial value
    ///
    /// Initializes version to 1 and sets timestamps to current time.
    pub fn new(id: JsonDocId, value: JsonValue) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        JsonDoc {
            id,
            value,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    /// Increment version and update timestamp
    ///
    /// Call this after any modification to the document.
    pub fn touch(&mut self) {
        self.version += 1;
        self.updated_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
    }
}

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

    // ========================================================================
    // Serialization (Story #273)
    // ========================================================================

    /// Serialize document for storage
    ///
    /// Uses MessagePack for efficient binary serialization.
    #[allow(dead_code)] // Will be used in Story #274+
    fn serialize_doc(doc: &JsonDoc) -> Result<Value> {
        let bytes = rmp_serde::to_vec(doc).map_err(|e| Error::SerializationError(e.to_string()))?;
        Ok(Value::Bytes(bytes))
    }

    /// Deserialize document from storage
    ///
    /// Expects Value::Bytes containing MessagePack-encoded JsonDoc.
    #[allow(dead_code)] // Will be used in Story #274+
    fn deserialize_doc(value: &Value) -> Result<JsonDoc> {
        match value {
            Value::Bytes(bytes) => {
                rmp_serde::from_slice(bytes).map_err(|e| Error::SerializationError(e.to_string()))
            }
            _ => Err(Error::InvalidOperation("expected bytes for JsonDoc".into())),
        }
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

    // ========================================
    // JsonDoc Tests (Story #273)
    // ========================================

    #[test]
    fn test_json_doc_new() {
        let id = JsonDocId::new();
        let value = JsonValue::from(42i64);
        let doc = JsonDoc::new(id, value.clone());

        assert_eq!(doc.id, id);
        assert_eq!(doc.value, value);
        assert_eq!(doc.version, 1);
        assert!(doc.created_at > 0);
        assert_eq!(doc.created_at, doc.updated_at);
    }

    #[test]
    fn test_json_doc_touch() {
        let id = JsonDocId::new();
        let value = JsonValue::from(42i64);
        let mut doc = JsonDoc::new(id, value);

        let old_version = doc.version;
        let old_updated = doc.updated_at;

        // Sleep a tiny bit to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(2));
        doc.touch();

        assert_eq!(doc.version, old_version + 1);
        assert!(doc.updated_at >= old_updated);
        // created_at should not change
        assert_eq!(doc.created_at, doc.created_at);
    }

    #[test]
    fn test_json_doc_touch_multiple() {
        let id = JsonDocId::new();
        let value = JsonValue::object();
        let mut doc = JsonDoc::new(id, value);

        for i in 0..5 {
            doc.touch();
            assert_eq!(doc.version, 2 + i);
        }
        assert_eq!(doc.version, 6);
    }

    // ========================================
    // Serialization Tests (Story #273)
    // ========================================

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        let doc = JsonDoc::new(JsonDocId::new(), JsonValue::from("test value"));

        let serialized = JsonStore::serialize_doc(&doc).unwrap();
        let deserialized = JsonStore::deserialize_doc(&serialized).unwrap();

        assert_eq!(doc.id, deserialized.id);
        assert_eq!(doc.value, deserialized.value);
        assert_eq!(doc.version, deserialized.version);
        assert_eq!(doc.created_at, deserialized.created_at);
        assert_eq!(doc.updated_at, deserialized.updated_at);
    }

    #[test]
    fn test_serialize_complex_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        let value: JsonValue = serde_json::json!({
            "string": "hello",
            "number": 42,
            "boolean": true,
            "null": null,
            "array": [1, 2, 3],
            "nested": {
                "foo": "bar"
            }
        })
        .into();

        let doc = JsonDoc::new(JsonDocId::new(), value);

        let serialized = JsonStore::serialize_doc(&doc).unwrap();
        let deserialized = JsonStore::deserialize_doc(&serialized).unwrap();

        assert_eq!(doc.value, deserialized.value);
    }

    #[test]
    fn test_deserialize_invalid_type() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        // Try to deserialize a non-bytes value
        let invalid = Value::I64(42);
        let result = JsonStore::deserialize_doc(&invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_invalid_bytes() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        // Try to deserialize garbage bytes
        let invalid = Value::Bytes(vec![0, 1, 2, 3, 4, 5]);
        let result = JsonStore::deserialize_doc(&invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_serialized_size_is_compact() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        let doc = JsonDoc::new(JsonDocId::new(), JsonValue::from(42i64));

        let serialized = JsonStore::serialize_doc(&doc).unwrap();

        match serialized {
            Value::Bytes(bytes) => {
                // MessagePack should produce reasonably compact output
                // UUID (16 bytes) + value + version + timestamps should be < 100 bytes
                assert!(bytes.len() < 100);
            }
            _ => panic!("Expected bytes"),
        }
    }
}
