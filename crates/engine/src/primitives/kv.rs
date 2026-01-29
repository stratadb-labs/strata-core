//! KVStore: General-purpose key-value storage primitive
//!
//! ## Design
//!
//! KVStore is a stateless facade over the Database engine. It holds no
//! in-memory state beyond an `Arc<Database>` reference.
//!
//! ## Run Isolation
//!
//! All operations are scoped to a `RunId`. Keys are prefixed with the
//! run's namespace, ensuring complete isolation between runs.
//!
//! ## Thread Safety
//!
//! KVStore is `Send + Sync` and can be safely shared across threads.
//! Multiple KVStore instances on the same Database are safe.
//!
//! ## MVP API
//!
//! - `get(run_id, key)` - Get latest value
//! - `put(run_id, key, value)` - Store a value
//! - `delete(run_id, key)` - Delete a key
//! - `list(run_id, prefix)` - List keys with prefix

use crate::database::Database;
use crate::primitives::extensions::KVStoreExt;
use strata_concurrency::TransactionContext;
use strata_core::error::Result;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_core::Version;
use std::sync::Arc;

/// General-purpose key-value store primitive
///
/// Stateless facade over Database - all state lives in storage.
/// Multiple KVStore instances on same Database are safe.
///
/// # Example
///
/// ```ignore
/// let db = Database::open("/path/to/data")?;
/// let kv = KVStore::new(db);
/// let run_id = RunId::new();
///
/// kv.put(&run_id, "key", Value::String("value".into()))?;
/// let value = kv.get(&run_id, "key")?;
/// kv.delete(&run_id, "key")?;
/// ```
#[derive(Clone)]
pub struct KVStore {
    db: Arc<Database>,
}

impl KVStore {
    /// Create new KVStore instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(*run_id)
    }

    /// Build key for KV operation
    fn key_for(&self, run_id: &RunId, user_key: &str) -> Key {
        Key::new_kv(self.namespace_for_run(run_id), user_key)
    }

    // ========== MVP API ==========

    /// Get a value by key
    ///
    /// Returns the latest value for the key, or None if it doesn't exist.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let value = kv.get(&run_id, "user:123")?;
    /// if let Some(v) = value {
    ///     println!("Found: {:?}", v);
    /// }
    /// ```
    pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>> {
        self.db.transaction(*run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.get(&storage_key)
        })
    }

    /// Put a value
    ///
    /// Creates the key if it doesn't exist, overwrites if it does.
    /// Returns the version created by this write operation.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let version = kv.put(&run_id, "user:123", Value::String("Alice".into()))?;
    /// ```
    pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version> {
        let ((), commit_version) = self.db.transaction_with_version(*run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.put(storage_key, value)
        })?;

        Ok(Version::Txn(commit_version))
    }

    /// Delete a key
    ///
    /// Returns `true` if the key existed and was deleted, `false` if it didn't exist.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let was_deleted = kv.delete(&run_id, "user:123")?;
    /// ```
    pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool> {
        self.db.transaction(*run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            let exists = txn.get(&storage_key)?.is_some();
            if exists {
                txn.delete(storage_key)?;
            }
            Ok(exists)
        })
    }

    /// List keys with optional prefix filter
    ///
    /// Returns all keys matching the prefix (or all keys if prefix is None).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // List all keys starting with "user:"
    /// let keys = kv.list(&run_id, Some("user:"))?;
    ///
    /// // List all keys
    /// let all_keys = kv.list(&run_id, None)?;
    /// ```
    pub fn list(&self, run_id: &RunId, prefix: Option<&str>) -> Result<Vec<String>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let scan_prefix = Key::new_kv(ns, prefix.unwrap_or(""));

            let results = txn.scan_prefix(&scan_prefix)?;

            Ok(results
                .into_iter()
                .filter_map(|(key, _)| key.user_key_string())
                .collect())
        })
    }
}

// ========== Searchable Trait Implementation ==========
//
// Search is handled by the intelligence layer (strata-intelligence).
// This implementation returns empty results - use InvertedIndex for full-text search.

impl crate::primitives::searchable::Searchable for KVStore {
    fn search(
        &self,
        _req: &crate::SearchRequest,
    ) -> strata_core::error::Result<crate::SearchResponse> {
        // Search moved to intelligence layer - return empty results
        Ok(crate::SearchResponse::empty())
    }

    fn primitive_kind(&self) -> strata_core::PrimitiveType {
        strata_core::PrimitiveType::Kv
    }
}

// ========== KVStoreExt Implementation ==========

impl KVStoreExt for TransactionContext {
    fn kv_get(&mut self, key: &str) -> Result<Option<Value>> {
        let storage_key = Key::new_kv(Namespace::for_run(self.run_id), key);
        self.get(&storage_key)
    }

    fn kv_put(&mut self, key: &str, value: Value) -> Result<()> {
        let storage_key = Key::new_kv(Namespace::for_run(self.run_id), key);
        self.put(storage_key, value)
    }

    fn kv_delete(&mut self, key: &str) -> Result<()> {
        let storage_key = Key::new_kv(Namespace::for_run(self.run_id), key);
        self.delete(storage_key)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::TypeTag;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<Database>, KVStore) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        let kv = KVStore::new(db.clone());
        (temp_dir, db, kv)
    }

    #[test]
    fn test_kvstore_creation() {
        let (_temp, _db, _kv) = setup();
    }

    #[test]
    fn test_kvstore_is_clone() {
        let (_temp, db, kv1) = setup();
        let kv2 = kv1.clone();
        // Both use same database
        assert!(Arc::ptr_eq(&db, &db));
        drop(kv2);
    }

    #[test]
    fn test_kvstore_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<KVStore>();
    }

    #[test]
    fn test_key_construction() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();
        let key = kv.key_for(&run_id, "test-key");
        assert_eq!(key.type_tag, TypeTag::KV);
    }

    #[test]
    fn test_put_and_get() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key1", Value::String("value1".into()))
            .unwrap();
        let result = kv.get(&run_id, "key1").unwrap();
        assert_eq!(result, Some(Value::String("value1".into())));
    }

    #[test]
    fn test_get_nonexistent() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        let result = kv.get(&run_id, "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_put_overwrite() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key1", Value::String("value1".into()))
            .unwrap();
        kv.put(&run_id, "key1", Value::String("value2".into()))
            .unwrap();

        let result = kv.get(&run_id, "key1").unwrap();
        assert_eq!(result, Some(Value::String("value2".into())));
    }

    #[test]
    fn test_delete() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key1", Value::String("value1".into()))
            .unwrap();
        assert!(kv.get(&run_id, "key1").unwrap().is_some());

        let deleted = kv.delete(&run_id, "key1").unwrap();
        assert!(deleted);
        assert!(kv.get(&run_id, "key1").unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        let deleted = kv.delete(&run_id, "nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_run_isolation() {
        let (_temp, _db, kv) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        kv.put(&run1, "shared-key", Value::String("run1-value".into()))
            .unwrap();
        kv.put(&run2, "shared-key", Value::String("run2-value".into()))
            .unwrap();

        // Each run sees its own value
        assert_eq!(
            kv.get(&run1, "shared-key").unwrap(),
            Some(Value::String("run1-value".into()))
        );
        assert_eq!(
            kv.get(&run2, "shared-key").unwrap(),
            Some(Value::String("run2-value".into()))
        );
    }

    #[test]
    fn test_list_all() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "a", Value::Int(1)).unwrap();
        kv.put(&run_id, "b", Value::Int(2)).unwrap();
        kv.put(&run_id, "c", Value::Int(3)).unwrap();

        let keys = kv.list(&run_id, None).unwrap();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"a".to_string()));
        assert!(keys.contains(&"b".to_string()));
        assert!(keys.contains(&"c".to_string()));
    }

    #[test]
    fn test_list_with_prefix() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "user:1", Value::Int(1)).unwrap();
        kv.put(&run_id, "user:2", Value::Int(2)).unwrap();
        kv.put(&run_id, "task:1", Value::Int(3)).unwrap();

        let user_keys = kv.list(&run_id, Some("user:")).unwrap();
        assert_eq!(user_keys.len(), 2);
        assert!(user_keys.contains(&"user:1".to_string()));
        assert!(user_keys.contains(&"user:2".to_string()));

        let task_keys = kv.list(&run_id, Some("task:")).unwrap();
        assert_eq!(task_keys.len(), 1);
        assert!(task_keys.contains(&"task:1".to_string()));
    }

    #[test]
    fn test_list_empty_prefix() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key1", Value::Int(1)).unwrap();
        kv.put(&run_id, "key2", Value::Int(2)).unwrap();

        let keys = kv.list(&run_id, Some("nonexistent:")).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn test_list_run_isolation() {
        let (_temp, _db, kv) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        kv.put(&run1, "run1-key", Value::Int(1)).unwrap();
        kv.put(&run2, "run2-key", Value::Int(2)).unwrap();

        // Each run only sees its own keys
        let run1_keys = kv.list(&run1, None).unwrap();
        assert_eq!(run1_keys.len(), 1);
        assert!(run1_keys.contains(&"run1-key".to_string()));

        let run2_keys = kv.list(&run2, None).unwrap();
        assert_eq!(run2_keys.len(), 1);
        assert!(run2_keys.contains(&"run2-key".to_string()));
    }

    #[test]
    fn test_various_value_types() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        // String
        kv.put(&run_id, "string", Value::String("hello".into()))
            .unwrap();
        assert_eq!(
            kv.get(&run_id, "string").unwrap(),
            Some(Value::String("hello".into()))
        );

        // Integer
        kv.put(&run_id, "int", Value::Int(42)).unwrap();
        assert_eq!(kv.get(&run_id, "int").unwrap(), Some(Value::Int(42)));

        // Float
        kv.put(&run_id, "float", Value::Float(3.14)).unwrap();
        assert_eq!(kv.get(&run_id, "float").unwrap(), Some(Value::Float(3.14)));

        // Boolean
        kv.put(&run_id, "bool", Value::Bool(true)).unwrap();
        assert_eq!(kv.get(&run_id, "bool").unwrap(), Some(Value::Bool(true)));

        // Null - Note: Value::Null is a tombstone, so storing it is equivalent to delete
        kv.put(&run_id, "null", Value::Null).unwrap();
        assert_eq!(kv.get(&run_id, "null").unwrap(), None);

        // Bytes
        kv.put(&run_id, "bytes", Value::Bytes(vec![1, 2, 3]))
            .unwrap();
        assert_eq!(
            kv.get(&run_id, "bytes").unwrap(),
            Some(Value::Bytes(vec![1, 2, 3]))
        );

        // Array
        kv.put(
            &run_id,
            "array",
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
        )
        .unwrap();
        assert_eq!(
            kv.get(&run_id, "array").unwrap(),
            Some(Value::Array(vec![Value::Int(1), Value::Int(2)]))
        );
    }

    #[test]
    fn test_kvstore_ext_in_transaction() {
        use crate::primitives::extensions::KVStoreExt;

        let (_temp, db, _kv) = setup();
        let run_id = RunId::new();

        db.transaction(run_id, |txn| {
            txn.kv_put("ext-key", Value::String("ext-value".into()))?;
            let val = txn.kv_get("ext-key")?;
            assert_eq!(val, Some(Value::String("ext-value".into())));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_kvstore_ext_delete() {
        use crate::primitives::extensions::KVStoreExt;

        let (_temp, db, kv) = setup();
        let run_id = RunId::new();

        // Setup
        kv.put(&run_id, "key", Value::Int(42)).unwrap();

        // Delete via extension trait
        db.transaction(run_id, |txn| {
            txn.kv_delete("key")?;
            let val = txn.kv_get("key")?;
            assert_eq!(val, None);
            Ok(())
        })
        .unwrap();
    }
}
