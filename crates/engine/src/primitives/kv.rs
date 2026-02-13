//! KVStore: General-purpose key-value storage primitive
//!
//! ## Design
//!
//! KVStore is a stateless facade over the Database engine. It holds no
//! in-memory state beyond an `Arc<Database>` reference.
//!
//! ## Branch Isolation
//!
//! All operations are scoped to a `BranchId`. Keys are prefixed with the
//! branch's namespace, ensuring complete isolation between branches.
//!
//! ## Thread Safety
//!
//! KVStore is `Send + Sync` and can be safely shared across threads.
//! Multiple KVStore instances on the same Database are safe.
//!
//! ## MVP API
//!
//! - `get(branch_id, key)` - Get latest value
//! - `put(branch_id, key, value)` - Store a value
//! - `delete(branch_id, key)` - Delete a key
//! - `list(branch_id, prefix)` - List keys with prefix

use crate::database::Database;
use crate::primitives::extensions::KVStoreExt;
use std::sync::Arc;
use strata_concurrency::TransactionContext;
use strata_core::types::{BranchId, Key, Namespace};
use strata_core::value::Value;
use strata_core::StrataResult;
use strata_core::{Version, VersionedHistory};

/// General-purpose key-value store primitive
///
/// Stateless facade over Database - all state lives in storage.
/// Multiple KVStore instances on same Database are safe.
///
/// # Example
///
/// ```text
/// let db = Database::open("/path/to/data")?;
/// let kv = KVStore::new(db);
/// let branch_id = BranchId::new();
///
/// kv.put(&branch_id, "default", "key", Value::String("value".into()))?;
/// let value = kv.get(&branch_id, "default", "key")?;
/// kv.delete(&branch_id, "default", "key")?;
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

    /// Build namespace for branch+space-scoped operations
    fn namespace_for(&self, branch_id: &BranchId, space: &str) -> Namespace {
        Namespace::for_branch_space(*branch_id, space)
    }

    /// Build key for KV operation
    fn key_for(&self, branch_id: &BranchId, space: &str, user_key: &str) -> Key {
        Key::new_kv(self.namespace_for(branch_id, space), user_key)
    }

    // ========== MVP API ==========

    /// Get a value by key
    ///
    /// Returns the latest value for the key, or None if it doesn't exist.
    ///
    /// # Example
    ///
    /// ```text
    /// let value = kv.get(&branch_id, "default", "user:123")?;
    /// if let Some(v) = value {
    ///     println!("Found: {:?}", v);
    /// }
    /// ```
    pub fn get(&self, branch_id: &BranchId, space: &str, key: &str) -> StrataResult<Option<Value>> {
        self.db.transaction(*branch_id, |txn| {
            let storage_key = self.key_for(branch_id, space, key);
            txn.get(&storage_key)
        })
    }

    /// Get a value with its version metadata.
    ///
    /// Uses a transaction to retrieve the latest value together with its
    /// version and timestamp, providing snapshot isolation.
    /// Returns `None` if the key doesn't exist.
    pub fn get_versioned(
        &self,
        branch_id: &BranchId,
        space: &str,
        key: &str,
    ) -> StrataResult<Option<strata_core::VersionedValue>> {
        self.db.transaction(*branch_id, |txn| {
            let storage_key = self.key_for(branch_id, space, key);
            txn.get_versioned(&storage_key)
        })
    }

    /// Get full version history for a key.
    ///
    /// Returns `None` if the key doesn't exist. Index with `[0]` = latest,
    /// `[1]` = previous, etc. Reads directly from storage (non-transactional).
    pub fn getv(
        &self,
        branch_id: &BranchId,
        space: &str,
        key: &str,
    ) -> StrataResult<Option<VersionedHistory<Value>>> {
        let storage_key = self.key_for(branch_id, space, key);
        let history = self.db.get_history(&storage_key, None, None)?;
        Ok(VersionedHistory::new(history))
    }

    /// Put a value
    ///
    /// Creates the key if it doesn't exist, overwrites if it does.
    /// Returns the version created by this write operation.
    ///
    /// # Example
    ///
    /// ```text
    /// let version = kv.put(&branch_id, "default", "user:123", Value::String("Alice".into()))?;
    /// ```
    pub fn put(
        &self,
        branch_id: &BranchId,
        space: &str,
        key: &str,
        value: Value,
    ) -> StrataResult<Version> {
        // Extract text for indexing before the value is consumed by the transaction
        let text_for_index = match &value {
            Value::String(s) => Some(s.clone()),
            Value::Null | Value::Bool(_) | Value::Bytes(_) => None,
            other => serde_json::to_string(other).ok(),
        };

        let ((), commit_version) = self.db.transaction_with_version(*branch_id, |txn| {
            let storage_key = self.key_for(branch_id, space, key);
            txn.put(storage_key, value)
        })?;

        // Update inverted index for BM25 search (zero overhead when disabled)
        if let Some(text) = text_for_index {
            let index = self.db.extension::<crate::search::InvertedIndex>()?;
            if index.is_enabled() {
                let entity_ref = crate::search::EntityRef::Kv {
                    branch_id: *branch_id,
                    key: key.to_string(),
                };
                index.index_document(&entity_ref, &text, None);
            }
        }

        Ok(Version::Txn(commit_version))
    }

    /// Delete a key
    ///
    /// Returns `true` if the key existed and was deleted, `false` if it didn't exist.
    ///
    /// # Example
    ///
    /// ```text
    /// let was_deleted = kv.delete(&branch_id, "default", "user:123")?;
    /// ```
    pub fn delete(&self, branch_id: &BranchId, space: &str, key: &str) -> StrataResult<bool> {
        self.db.transaction(*branch_id, |txn| {
            let storage_key = self.key_for(branch_id, space, key);
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
    /// ```text
    /// // List all keys starting with "user:"
    /// let keys = kv.list(&branch_id, "default", Some("user:"))?;
    ///
    /// // List all keys
    /// let all_keys = kv.list(&branch_id, "default", None)?;
    /// ```
    pub fn list(
        &self,
        branch_id: &BranchId,
        space: &str,
        prefix: Option<&str>,
    ) -> StrataResult<Vec<String>> {
        self.db.transaction(*branch_id, |txn| {
            let ns = self.namespace_for(branch_id, space);
            let scan_prefix = Key::new_kv(ns, prefix.unwrap_or(""));

            let results = txn.scan_prefix(&scan_prefix)?;

            Ok(results
                .into_iter()
                .filter_map(|(key, _)| key.user_key_string())
                .collect())
        })
    }

    // ========== Time-Travel API ==========

    /// Get a value by key as of a past timestamp (microseconds since epoch).
    ///
    /// Returns the latest value whose commit timestamp <= as_of_ts, or None.
    /// This is a non-transactional read directly from the storage version chain.
    pub fn get_at(
        &self,
        branch_id: &BranchId,
        space: &str,
        key: &str,
        as_of_ts: u64,
    ) -> StrataResult<Option<Value>> {
        let storage_key = self.key_for(branch_id, space, key);
        let result = self.db.get_at_timestamp(&storage_key, as_of_ts)?;
        Ok(result.map(|vv| vv.value))
    }

    /// List keys as of a past timestamp.
    ///
    /// Returns keys whose values existed at the given timestamp.
    pub fn list_at(
        &self,
        branch_id: &BranchId,
        space: &str,
        prefix: Option<&str>,
        as_of_ts: u64,
    ) -> StrataResult<Vec<String>> {
        let ns = self.namespace_for(branch_id, space);
        let scan_prefix = Key::new_kv(ns, prefix.unwrap_or(""));
        let results = self.db.scan_prefix_at_timestamp(&scan_prefix, as_of_ts)?;
        Ok(results
            .into_iter()
            .filter_map(|(key, _)| key.user_key_string())
            .collect())
    }
}

// ========== Searchable Trait Implementation ==========
//
// Search is handled by the intelligence layer (strata-intelligence).
// This implementation returns empty results - use InvertedIndex for full-text search.

impl crate::search::Searchable for KVStore {
    fn search(
        &self,
        req: &crate::SearchRequest,
    ) -> strata_core::StrataResult<crate::SearchResponse> {
        use crate::search::{
            build_search_response_with_index, EntityRef, InvertedIndex, SearchCandidate,
        };
        use std::collections::HashMap;
        use std::time::Instant;

        let start = Instant::now();
        let index = self.db.extension::<InvertedIndex>()?;

        // If the index is disabled or empty, return early
        if !index.is_enabled() || index.total_docs() == 0 {
            return Ok(crate::SearchResponse::empty());
        }

        // Tokenize the query and collect matching KV candidates from posting
        // lists, accumulating per-candidate term frequencies so BM25 scoring
        // can skip re-tokenizing (and re-stemming) the full document body.
        let query_terms = crate::search::tokenize(&req.query);

        // key -> (term_freqs, doc_len)
        let mut candidate_tfs: HashMap<String, (HashMap<String, u32>, u32)> = HashMap::new();

        for term in &query_terms {
            if let Some(posting_list) = index.lookup(term) {
                for entry in &posting_list.entries {
                    if let EntityRef::Kv {
                        branch_id,
                        ref key,
                    } = entry.doc_ref
                    {
                        if branch_id == req.branch_id {
                            let tf_entry = candidate_tfs
                                .entry(key.clone())
                                .or_insert_with(|| (HashMap::new(), entry.doc_len));
                            tf_entry.0.insert(term.clone(), entry.tf);
                        }
                    }
                }
            }
        }

        // Fetch text (for snippets) and build candidates with pre-computed TFs
        let candidates: Vec<SearchCandidate> = candidate_tfs
            .into_iter()
            .filter_map(|(key, (tf_map, doc_len))| {
                if let Ok(Some(value)) = self.get(&req.branch_id, "default", &key) {
                    let text = match &value {
                        strata_core::value::Value::String(s) => s.clone(),
                        other => serde_json::to_string(other).unwrap_or_default(),
                    };
                    let entity_ref = EntityRef::Kv {
                        branch_id: req.branch_id,
                        key,
                    };
                    Some(
                        SearchCandidate::new(entity_ref, text, None)
                            .with_precomputed_tf(tf_map, doc_len),
                    )
                } else {
                    None
                }
            })
            .collect();

        let elapsed = start.elapsed().as_micros() as u64;
        Ok(build_search_response_with_index(
            candidates,
            &req.query,
            req.k,
            false,
            elapsed,
            Some(&index),
        ))
    }

    fn primitive_kind(&self) -> strata_core::PrimitiveType {
        strata_core::PrimitiveType::Kv
    }
}

// ========== KVStoreExt Implementation ==========

impl KVStoreExt for TransactionContext {
    fn kv_get(&mut self, key: &str) -> StrataResult<Option<Value>> {
        let storage_key = Key::new_kv(Namespace::for_branch(self.branch_id), key);
        self.get(&storage_key)
    }

    fn kv_put(&mut self, key: &str, value: Value) -> StrataResult<()> {
        let storage_key = Key::new_kv(Namespace::for_branch(self.branch_id), key);
        self.put(storage_key, value)
    }

    fn kv_delete(&mut self, key: &str) -> StrataResult<()> {
        let storage_key = Key::new_kv(Namespace::for_branch(self.branch_id), key);
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
        let branch_id = BranchId::new();
        let key = kv.key_for(&branch_id, "default", "test-key");
        assert_eq!(key.type_tag, TypeTag::KV);
    }

    #[test]
    fn test_put_and_get() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        kv.put(
            &branch_id,
            "default",
            "key1",
            Value::String("value1".into()),
        )
        .unwrap();
        let result = kv.get(&branch_id, "default", "key1").unwrap();
        assert_eq!(result, Some(Value::String("value1".into())));
    }

    #[test]
    fn test_get_nonexistent() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        let result = kv.get(&branch_id, "default", "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_put_overwrite() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        kv.put(
            &branch_id,
            "default",
            "key1",
            Value::String("value1".into()),
        )
        .unwrap();
        kv.put(
            &branch_id,
            "default",
            "key1",
            Value::String("value2".into()),
        )
        .unwrap();

        let result = kv.get(&branch_id, "default", "key1").unwrap();
        assert_eq!(result, Some(Value::String("value2".into())));
    }

    #[test]
    fn test_delete() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        kv.put(
            &branch_id,
            "default",
            "key1",
            Value::String("value1".into()),
        )
        .unwrap();
        assert!(kv.get(&branch_id, "default", "key1").unwrap().is_some());

        let deleted = kv.delete(&branch_id, "default", "key1").unwrap();
        assert!(deleted);
        assert!(kv.get(&branch_id, "default", "key1").unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        let deleted = kv.delete(&branch_id, "default", "nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_branch_isolation() {
        let (_temp, _db, kv) = setup();
        let branch1 = BranchId::new();
        let branch2 = BranchId::new();

        kv.put(
            &branch1,
            "default",
            "shared-key",
            Value::String("branch1-value".into()),
        )
        .unwrap();
        kv.put(
            &branch2,
            "default",
            "shared-key",
            Value::String("branch2-value".into()),
        )
        .unwrap();

        // Each branch sees its own value
        assert_eq!(
            kv.get(&branch1, "default", "shared-key").unwrap(),
            Some(Value::String("branch1-value".into()))
        );
        assert_eq!(
            kv.get(&branch2, "default", "shared-key").unwrap(),
            Some(Value::String("branch2-value".into()))
        );
    }

    #[test]
    fn test_list_all() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        kv.put(&branch_id, "default", "a", Value::Int(1)).unwrap();
        kv.put(&branch_id, "default", "b", Value::Int(2)).unwrap();
        kv.put(&branch_id, "default", "c", Value::Int(3)).unwrap();

        let keys = kv.list(&branch_id, "default", None).unwrap();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"a".to_string()));
        assert!(keys.contains(&"b".to_string()));
        assert!(keys.contains(&"c".to_string()));
    }

    #[test]
    fn test_list_with_prefix() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        kv.put(&branch_id, "default", "user:1", Value::Int(1))
            .unwrap();
        kv.put(&branch_id, "default", "user:2", Value::Int(2))
            .unwrap();
        kv.put(&branch_id, "default", "task:1", Value::Int(3))
            .unwrap();

        let user_keys = kv.list(&branch_id, "default", Some("user:")).unwrap();
        assert_eq!(user_keys.len(), 2);
        assert!(user_keys.contains(&"user:1".to_string()));
        assert!(user_keys.contains(&"user:2".to_string()));

        let task_keys = kv.list(&branch_id, "default", Some("task:")).unwrap();
        assert_eq!(task_keys.len(), 1);
        assert!(task_keys.contains(&"task:1".to_string()));
    }

    #[test]
    fn test_list_empty_prefix() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        kv.put(&branch_id, "default", "key1", Value::Int(1))
            .unwrap();
        kv.put(&branch_id, "default", "key2", Value::Int(2))
            .unwrap();

        let keys = kv
            .list(&branch_id, "default", Some("nonexistent:"))
            .unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn test_list_branch_isolation() {
        let (_temp, _db, kv) = setup();
        let branch1 = BranchId::new();
        let branch2 = BranchId::new();

        kv.put(&branch1, "default", "branch1-key", Value::Int(1))
            .unwrap();
        kv.put(&branch2, "default", "branch2-key", Value::Int(2))
            .unwrap();

        // Each branch only sees its own keys
        let branch1_keys = kv.list(&branch1, "default", None).unwrap();
        assert_eq!(branch1_keys.len(), 1);
        assert!(branch1_keys.contains(&"branch1-key".to_string()));

        let branch2_keys = kv.list(&branch2, "default", None).unwrap();
        assert_eq!(branch2_keys.len(), 1);
        assert!(branch2_keys.contains(&"branch2-key".to_string()));
    }

    #[test]
    fn test_various_value_types() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        // String
        kv.put(
            &branch_id,
            "default",
            "string",
            Value::String("hello".into()),
        )
        .unwrap();
        assert_eq!(
            kv.get(&branch_id, "default", "string").unwrap(),
            Some(Value::String("hello".into()))
        );

        // Integer
        kv.put(&branch_id, "default", "int", Value::Int(42))
            .unwrap();
        assert_eq!(
            kv.get(&branch_id, "default", "int").unwrap(),
            Some(Value::Int(42))
        );

        // Float
        kv.put(&branch_id, "default", "float", Value::Float(3.14))
            .unwrap();
        assert_eq!(
            kv.get(&branch_id, "default", "float").unwrap(),
            Some(Value::Float(3.14))
        );

        // Boolean
        kv.put(&branch_id, "default", "bool", Value::Bool(true))
            .unwrap();
        assert_eq!(
            kv.get(&branch_id, "default", "bool").unwrap(),
            Some(Value::Bool(true))
        );

        // Null - Value::Null should be storable and round-trip correctly
        kv.put(&branch_id, "default", "null", Value::Null).unwrap();
        let result = kv.get(&branch_id, "default", "null").unwrap();
        assert!(result.is_some(), "Value::Null should be storable");
        assert_eq!(result.unwrap(), Value::Null);

        // Bytes
        kv.put(&branch_id, "default", "bytes", Value::Bytes(vec![1, 2, 3]))
            .unwrap();
        assert_eq!(
            kv.get(&branch_id, "default", "bytes").unwrap(),
            Some(Value::Bytes(vec![1, 2, 3]))
        );

        // Array
        kv.put(
            &branch_id,
            "default",
            "array",
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
        )
        .unwrap();
        assert_eq!(
            kv.get(&branch_id, "default", "array").unwrap(),
            Some(Value::Array(vec![Value::Int(1), Value::Int(2)]))
        );
    }

    #[test]
    fn test_kvstore_ext_in_transaction() {
        use crate::primitives::extensions::KVStoreExt;

        let (_temp, db, _kv) = setup();
        let branch_id = BranchId::new();

        db.transaction(branch_id, |txn| {
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
        let branch_id = BranchId::new();

        // Setup
        kv.put(&branch_id, "default", "key", Value::Int(42))
            .unwrap();

        // Delete via extension trait
        db.transaction(branch_id, |txn| {
            txn.kv_delete("key")?;
            let val = txn.kv_get("key")?;
            assert_eq!(val, None);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_get_versioned_returns_version_info() {
        let (_temp, _db, kv) = setup();
        let branch_id = BranchId::new();

        let version = kv
            .put(&branch_id, "default", "vkey", Value::Int(99))
            .unwrap();
        let vv = kv
            .get_versioned(&branch_id, "default", "vkey")
            .unwrap()
            .unwrap();
        assert_eq!(vv.value, Value::Int(99));
        assert_eq!(vv.version, version);
    }

    #[test]
    fn test_get_versioned_snapshot_isolation() {
        let (_temp, db, kv) = setup();
        let branch_id = BranchId::new();

        kv.put(&branch_id, "default", "iso_key", Value::Int(1))
            .unwrap();

        // Start a manual transaction, read, then check the versioned read
        // is consistent even if a concurrent write happens
        let mut txn = db.begin_transaction(branch_id);
        let storage_key =
            strata_core::types::Key::new_kv(Namespace::for_branch(branch_id), "iso_key");
        let vv = txn.get_versioned(&storage_key).unwrap().unwrap();
        assert_eq!(vv.value, Value::Int(1));

        // Concurrent write after our snapshot
        kv.put(&branch_id, "default", "iso_key", Value::Int(2))
            .unwrap();

        // Re-read within same transaction should still see old value
        let vv2 = txn.get_versioned(&storage_key).unwrap().unwrap();
        assert_eq!(vv2.value, Value::Int(1));

        db.end_transaction(txn);
    }
}
