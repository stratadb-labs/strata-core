//! TransactionOps trait - unified primitive operations
//!
//! This trait expresses Invariant 3: Everything is Transactional.
//! Every primitive's operations are accessible through this trait,
//! enabling cross-primitive atomic operations.
//!
//! ## Design Principles
//!
//! 1. **Reads are `&self`**: Read operations never modify state
//! 2. **Writes are `&mut self`**: Write operations require exclusive access
//! 3. **All operations return `Result<T, StrataError>`**: Consistent error handling
//! 4. **All reads return `Versioned<T>`**: Version information is never lost
//! 5. **All writes return `Version`**: Every mutation produces a version
//!
//! ## Usage
//!
//! ```rust,ignore
//! db.transaction(&run_id, |txn| {
//!     // Read from KV
//!     let config = txn.kv_get("config")?;
//!
//!     // Write to Event
//!     let event_version = txn.event_append("config_read", json!({}))?;
//!
//!     // Update State
//!     txn.state_set("last_event", Value::from(event_version.as_u64()))?;
//!
//!     Ok(())
//! })?;
//! ```

use strata_core::{
    Event, JsonDocId, JsonPath, JsonValue, MetadataFilter, RunMetadata, RunStatus, State,
    StrataError, Value, VectorEntry, VectorMatch, Version, Versioned,
};

/// Operations available within a transaction
///
/// This trait expresses Invariant 3: Everything is Transactional.
/// Every primitive's operations are accessible through this trait,
/// enabling cross-primitive atomic operations.
///
/// ## Phase 2 Implementation
///
/// Phase 2 implements KV and EventLog operations. Other primitive
/// operations return `unimplemented!()` and will be wired in later phases:
/// - Phase 3: State
/// - Phase 4: Json + Vector
/// - Phase 5: Run operations (finalize)
pub trait TransactionOps {
    // =========================================================================
    // KV Operations (Phase 2)
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
    fn kv_list(&self, prefix: Option<&str>) -> Result<Vec<String>, StrataError>;

    // =========================================================================
    // Event Operations (Phase 2)
    // =========================================================================

    /// Append an event to the log
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Version, StrataError>;

    /// Read an event by sequence number
    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>, StrataError>;

    /// Read a range of events [start, end)
    fn event_range(&self, start: u64, end: u64) -> Result<Vec<Versioned<Event>>, StrataError>;

    /// Get current event count (length of the log)
    fn event_len(&self) -> Result<u64, StrataError>;

    // =========================================================================
    // State Operations (Phase 3)
    // =========================================================================

    /// Read a state cell
    fn state_read(&self, name: &str) -> Result<Option<Versioned<State>>, StrataError>;

    /// Initialize a state cell (fails if exists)
    fn state_init(&mut self, name: &str, value: Value) -> Result<Version, StrataError>;

    /// Compare-and-swap a state cell
    fn state_cas(
        &mut self,
        name: &str,
        expected_version: u64,
        value: Value,
    ) -> Result<Version, StrataError>;

    /// Delete a state cell
    fn state_delete(&mut self, name: &str) -> Result<bool, StrataError>;

    /// Check if a state cell exists
    fn state_exists(&self, name: &str) -> Result<bool, StrataError>;

    // =========================================================================
    // Json Operations (Phase 4)
    // =========================================================================

    /// Create a JSON document
    fn json_create(&mut self, doc_id: &JsonDocId, value: JsonValue) -> Result<Version, StrataError>;

    /// Get an entire JSON document
    fn json_get(&self, doc_id: &JsonDocId) -> Result<Option<Versioned<JsonValue>>, StrataError>;

    /// Get a value at a path within a JSON document
    fn json_get_path(
        &self,
        doc_id: &JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<JsonValue>, StrataError>;

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

    /// Destroy a JSON document (same as delete, for API consistency)
    fn json_destroy(&mut self, doc_id: &JsonDocId) -> Result<bool, StrataError>;

    // =========================================================================
    // Vector Operations (Phase 4)
    // =========================================================================

    /// Insert a vector into a collection
    fn vector_insert(
        &mut self,
        collection: &str,
        key: &str,
        embedding: &[f32],
        metadata: Option<Value>,
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
    // Run Operations (Phase 5 - Limited, runs are meta-level)
    // =========================================================================

    /// Get run metadata (the current run)
    fn run_metadata(&self) -> Result<Option<Versioned<RunMetadata>>, StrataError>;

    /// Update run status
    fn run_update_status(&mut self, status: RunStatus) -> Result<Version, StrataError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock implementation of TransactionOps for testing trait properties
    struct MockTransactionOps {
        kv_data: std::collections::HashMap<String, Value>,
        event_count: u64,
    }

    impl MockTransactionOps {
        fn new() -> Self {
            Self {
                kv_data: std::collections::HashMap::new(),
                event_count: 0,
            }
        }
    }

    impl TransactionOps for MockTransactionOps {
        fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError> {
            Ok(self.kv_data.get(key).map(|v| Versioned::new(v.clone(), Version::txn(1))))
        }

        fn kv_put(&mut self, key: &str, value: Value) -> Result<Version, StrataError> {
            self.kv_data.insert(key.to_string(), value);
            Ok(Version::txn(1))
        }

        fn kv_delete(&mut self, key: &str) -> Result<bool, StrataError> {
            Ok(self.kv_data.remove(key).is_some())
        }

        fn kv_exists(&self, key: &str) -> Result<bool, StrataError> {
            Ok(self.kv_data.contains_key(key))
        }

        fn kv_list(&self, prefix: Option<&str>) -> Result<Vec<String>, StrataError> {
            let keys: Vec<_> = self.kv_data.keys()
                .filter(|k| prefix.is_none() || k.starts_with(prefix.unwrap()))
                .cloned()
                .collect();
            Ok(keys)
        }

        fn event_append(&mut self, _event_type: &str, _payload: Value) -> Result<Version, StrataError> {
            self.event_count += 1;
            Ok(Version::seq(self.event_count))
        }

        fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>, StrataError> {
            // Return None for simplicity - Event struct is complex
            if sequence == 0 || sequence > self.event_count {
                return Ok(None);
            }
            // For testing purposes, we'll return None rather than construct a complex Event
            // The trait behavior is still tested through event_append and event_len
            Ok(None)
        }

        fn event_range(&self, _start: u64, _end: u64) -> Result<Vec<Versioned<Event>>, StrataError> {
            // Return empty for simplicity
            Ok(Vec::new())
        }

        fn event_len(&self) -> Result<u64, StrataError> {
            Ok(self.event_count)
        }

        // State operations - return not implemented error for mock
        fn state_read(&self, _name: &str) -> Result<Option<Versioned<State>>, StrataError> {
            Err(StrataError::Internal { message: "state_read not implemented in mock".to_string() })
        }

        fn state_init(&mut self, _name: &str, _value: Value) -> Result<Version, StrataError> {
            Err(StrataError::Internal { message: "state_init not implemented in mock".to_string() })
        }

        fn state_cas(&mut self, _name: &str, _expected: u64, _value: Value) -> Result<Version, StrataError> {
            Err(StrataError::Internal { message: "state_cas not implemented in mock".to_string() })
        }

        fn state_delete(&mut self, _name: &str) -> Result<bool, StrataError> {
            Err(StrataError::Internal { message: "state_delete not implemented in mock".to_string() })
        }

        fn state_exists(&self, _name: &str) -> Result<bool, StrataError> {
            Err(StrataError::Internal { message: "state_exists not implemented in mock".to_string() })
        }

        // Json operations
        fn json_create(&mut self, _doc_id: &JsonDocId, _value: JsonValue) -> Result<Version, StrataError> {
            Err(StrataError::Internal { message: "json_create not implemented in mock".to_string() })
        }

        fn json_get(&self, _doc_id: &JsonDocId) -> Result<Option<Versioned<JsonValue>>, StrataError> {
            Err(StrataError::Internal { message: "json_get not implemented in mock".to_string() })
        }

        fn json_get_path(&self, _doc_id: &JsonDocId, _path: &JsonPath) -> Result<Option<JsonValue>, StrataError> {
            Err(StrataError::Internal { message: "json_get_path not implemented in mock".to_string() })
        }

        fn json_set(&mut self, _doc_id: &JsonDocId, _path: &JsonPath, _value: JsonValue) -> Result<Version, StrataError> {
            Err(StrataError::Internal { message: "json_set not implemented in mock".to_string() })
        }

        fn json_delete(&mut self, _doc_id: &JsonDocId) -> Result<bool, StrataError> {
            Err(StrataError::Internal { message: "json_delete not implemented in mock".to_string() })
        }

        fn json_exists(&self, _doc_id: &JsonDocId) -> Result<bool, StrataError> {
            Err(StrataError::Internal { message: "json_exists not implemented in mock".to_string() })
        }

        fn json_destroy(&mut self, _doc_id: &JsonDocId) -> Result<bool, StrataError> {
            Err(StrataError::Internal { message: "json_destroy not implemented in mock".to_string() })
        }

        // Vector operations
        fn vector_insert(&mut self, _collection: &str, _key: &str, _embedding: &[f32], _metadata: Option<Value>) -> Result<Version, StrataError> {
            Err(StrataError::Internal { message: "vector_insert not implemented in mock".to_string() })
        }

        fn vector_get(&self, _collection: &str, _key: &str) -> Result<Option<Versioned<VectorEntry>>, StrataError> {
            Err(StrataError::Internal { message: "vector_get not implemented in mock".to_string() })
        }

        fn vector_delete(&mut self, _collection: &str, _key: &str) -> Result<bool, StrataError> {
            Err(StrataError::Internal { message: "vector_delete not implemented in mock".to_string() })
        }

        fn vector_search(&self, _collection: &str, _query: &[f32], _k: usize, _filter: Option<MetadataFilter>) -> Result<Vec<VectorMatch>, StrataError> {
            Err(StrataError::Internal { message: "vector_search not implemented in mock".to_string() })
        }

        fn vector_exists(&self, _collection: &str, _key: &str) -> Result<bool, StrataError> {
            Err(StrataError::Internal { message: "vector_exists not implemented in mock".to_string() })
        }

        // Run operations
        fn run_metadata(&self) -> Result<Option<Versioned<RunMetadata>>, StrataError> {
            Err(StrataError::Internal { message: "run_metadata not implemented in mock".to_string() })
        }

        fn run_update_status(&mut self, _status: RunStatus) -> Result<Version, StrataError> {
            Err(StrataError::Internal { message: "run_update_status not implemented in mock".to_string() })
        }
    }

    // ========== Object Safety Tests ==========

    /// Verify trait is object-safe by creating a trait object
    fn accept_dyn_transaction_ops(_ops: &dyn TransactionOps) {}

    fn accept_mut_dyn_transaction_ops(_ops: &mut dyn TransactionOps) {}

    fn accept_boxed_dyn_transaction_ops(_ops: Box<dyn TransactionOps>) {}

    #[test]
    fn test_trait_is_object_safe_ref() {
        let ops = MockTransactionOps::new();
        accept_dyn_transaction_ops(&ops);
    }

    #[test]
    fn test_trait_is_object_safe_mut_ref() {
        let mut ops = MockTransactionOps::new();
        accept_mut_dyn_transaction_ops(&mut ops);
    }

    #[test]
    fn test_trait_is_object_safe_boxed() {
        let ops = MockTransactionOps::new();
        accept_boxed_dyn_transaction_ops(Box::new(ops));
    }

    // ========== Read Operations Through Trait Object ==========

    #[test]
    fn test_kv_operations_through_trait_object() {
        let mut ops: Box<dyn TransactionOps> = Box::new(MockTransactionOps::new());

        // Put through trait object
        let version = ops.kv_put("key1", Value::Int(42)).unwrap();
        assert_eq!(version.as_u64(), 1);

        // Get through trait object
        let result = ops.kv_get("key1").unwrap();
        assert!(result.is_some());
        let versioned = result.unwrap();
        assert_eq!(versioned.value, Value::Int(42));

        // Exists through trait object
        assert!(ops.kv_exists("key1").unwrap());
        assert!(!ops.kv_exists("nonexistent").unwrap());

        // Delete through trait object
        let deleted = ops.kv_delete("key1").unwrap();
        assert!(deleted);
        assert!(!ops.kv_exists("key1").unwrap());
    }

    #[test]
    fn test_event_operations_through_trait_object() {
        let mut ops: Box<dyn TransactionOps> = Box::new(MockTransactionOps::new());

        // Append events
        let v1 = ops.event_append("UserCreated", Value::String("alice".into())).unwrap();
        let v2 = ops.event_append("UserUpdated", Value::String("bob".into())).unwrap();

        // Check versions are sequential
        assert_eq!(v1.as_u64(), 1);
        assert_eq!(v2.as_u64(), 2);

        // Check length
        assert_eq!(ops.event_len().unwrap(), 2);

        // Non-existent event (beyond the event count)
        assert!(ops.event_read(999).unwrap().is_none());
    }

    #[test]
    fn test_kv_list_through_trait_object() {
        let mut ops: Box<dyn TransactionOps> = Box::new(MockTransactionOps::new());

        ops.kv_put("user:1", Value::Int(1)).unwrap();
        ops.kv_put("user:2", Value::Int(2)).unwrap();
        ops.kv_put("config:a", Value::Int(3)).unwrap();

        // List all
        let all_keys = ops.kv_list(None).unwrap();
        assert_eq!(all_keys.len(), 3);

        // List with prefix
        let user_keys = ops.kv_list(Some("user:")).unwrap();
        assert_eq!(user_keys.len(), 2);
        assert!(user_keys.iter().all(|k| k.starts_with("user:")));
    }

    // ========== Unimplemented Operations Return Proper Errors ==========

    #[test]
    fn test_unimplemented_operations_return_errors() {
        let ops: Box<dyn TransactionOps> = Box::new(MockTransactionOps::new());

        // State operations should return unimplemented error
        let result = ops.state_read("test");
        assert!(result.is_err());

        let result = ops.state_exists("test");
        assert!(result.is_err());

        // Json operations should return unimplemented error
        let doc_id = JsonDocId::new();
        let result = ops.json_get(&doc_id);
        assert!(result.is_err());

        // Vector operations should return unimplemented error
        let result = ops.vector_exists("collection", "key");
        assert!(result.is_err());

        // Run operations should return unimplemented error
        let result = ops.run_metadata();
        assert!(result.is_err());
    }

    // ========== Trait Method Signatures ==========

    #[test]
    fn test_read_methods_take_shared_ref() {
        // This test verifies that read methods use &self (not &mut self)
        // by calling them on an immutable reference
        let ops = MockTransactionOps::new();
        let ops_ref: &dyn TransactionOps = &ops;

        // All these should compile with &self
        let _ = ops_ref.kv_get("key");
        let _ = ops_ref.kv_exists("key");
        let _ = ops_ref.kv_list(None);
        let _ = ops_ref.event_read(1);
        let _ = ops_ref.event_range(1, 10);
        let _ = ops_ref.event_len();
    }

    #[test]
    fn test_write_methods_take_mutable_ref() {
        // This test verifies that write methods use &mut self
        let mut ops = MockTransactionOps::new();
        let ops_mut: &mut dyn TransactionOps = &mut ops;

        // All these should compile with &mut self
        let _ = ops_mut.kv_put("key", Value::Int(1));
        let _ = ops_mut.kv_delete("key");
        let _ = ops_mut.event_append("test", Value::Null);
    }
}
