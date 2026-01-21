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
//! ## API
//!
//! - **Single-Operation API**: `get`, `put`, `put_with_ttl`, `delete`, `exists`
//!   Each operation runs in its own implicit transaction.
//!
//! - **Multi-Operation API**: `transaction` with `KVTransaction`
//!   Multiple operations run atomically in a single transaction.
//!
//! - **List Operations**: `list`, `list_with_values`
//!   Scan keys with optional prefix filtering.

use crate::extensions::KVStoreExt;
use in_mem_concurrency::TransactionContext;
use in_mem_core::error::Result;
use in_mem_core::types::{Key, Namespace, RunId};
use in_mem_core::value::Value;
use in_mem_core::{Version, Versioned};
use in_mem_engine::Database;
use std::sync::Arc;
use std::time::Duration;

/// General-purpose key-value store primitive
///
/// Stateless facade over Database - all state lives in storage.
/// Multiple KVStore instances on same Database are safe.
///
/// # Example
///
/// ```ignore
/// use in_mem_primitives::KVStore;
/// use in_mem_engine::Database;
/// use in_mem_core::types::RunId;
/// use in_mem_core::value::Value;
///
/// let db = Arc::new(Database::open("/path/to/data")?);
/// let kv = KVStore::new(db);
/// let run_id = RunId::new();
///
/// // Simple operations
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

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(*run_id)
    }

    /// Build key for KV operation
    fn key_for(&self, run_id: &RunId, user_key: &str) -> Key {
        Key::new_kv(self.namespace_for_run(run_id), user_key)
    }

    // ========== Single-Operation API (Implicit Transactions) ==========

    /// Get a value by key (FAST PATH)
    ///
    /// Returns `Versioned<Value>` containing the value, its version, and timestamp.
    ///
    /// Bypasses full transaction overhead:
    /// - No transaction object allocation
    /// - No read-set recording
    /// - No commit validation
    /// - No WAL append
    ///
    /// PRESERVES:
    /// - Snapshot isolation (consistent view)
    /// - Run isolation (key prefixing)
    ///
    /// # Performance Contract
    /// - < 10µs (target: <5µs)
    /// - Zero allocations (except return value clone)
    ///
    /// # Invariant
    /// Observationally equivalent to transaction-based read.
    /// Returns the same value that a read-only transaction started
    /// at the same moment would return.
    ///
    /// Returns `None` if the key doesn't exist.
    ///
    /// # M9 Contract
    /// - Returns `Versioned<Value>` with version information
    /// - Version is `Version::TxnId(commit_version)` from storage
    pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>> {
        use in_mem_core::traits::SnapshotView;

        // Fast path: direct snapshot read
        let snapshot = self.db.storage().create_snapshot();
        let storage_key = self.key_for(run_id, key);

        // M9: Return full Versioned<Value> instead of just value
        Ok(snapshot.get(&storage_key)?)
    }

    /// Get value only, discarding version (DEPRECATED)
    ///
    /// For backwards compatibility. New code should use `get()` and access `.value`.
    #[deprecated(since = "0.9.0", note = "Use get() and access .value directly")]
    pub fn get_value(&self, run_id: &RunId, key: &str) -> Result<Option<Value>> {
        Ok(self.get(run_id, key)?.map(|v| v.value))
    }

    /// Get a value using full transaction (for comparison/fallback)
    ///
    /// Use this when you need transaction semantics, e.g.:
    /// - Read-modify-write patterns
    /// - When you need read-set tracking for conflict detection
    ///
    /// For simple reads, prefer `get()` which is faster.
    pub fn get_in_transaction(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>> {
        self.db.transaction(*run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            let result = txn.get(&storage_key)?;
            // M9: Wrap in Versioned with transaction snapshot version
            Ok(result.map(|v| Versioned::new(v, Version::TxnId(txn.start_version))))
        })
    }

    /// Put a value (M9: Returns version)
    ///
    /// Creates the key if it doesn't exist, overwrites if it does.
    /// Returns the version created by this write operation.
    ///
    /// # M9 Contract
    /// - Returns `Version::TxnId(commit_version)` on success
    pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version> {
        let ((), commit_version) = self.db.transaction_with_version(*run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.put(storage_key, value)
        })?;
        Ok(Version::TxnId(commit_version))
    }

    /// Put a value without returning version (DEPRECATED)
    ///
    /// For backwards compatibility. New code should use `put()` which returns Version.
    #[deprecated(since = "0.9.0", note = "Use put() which returns Version")]
    pub fn put_no_version(&self, run_id: &RunId, key: &str, value: Value) -> Result<()> {
        self.db.transaction(*run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            txn.put(storage_key, value)
        })
    }

    /// Put a value with TTL (M9: Returns version)
    ///
    /// Note: TTL metadata is stored but cleanup is deferred to M4 background tasks.
    /// Reads will return expired values until cleanup runs.
    ///
    /// # M9 Contract
    /// - Returns `Version::TxnId(commit_version)` on success
    pub fn put_with_ttl(
        &self,
        run_id: &RunId,
        key: &str,
        value: Value,
        ttl: Duration,
    ) -> Result<Version> {
        let ((), commit_version) = self.db.transaction_with_version(*run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            let expires_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
                + ttl.as_millis() as i64;

            // Store value with expiration metadata
            let value_with_ttl = Value::Map(std::collections::HashMap::from([
                ("value".to_string(), value),
                ("expires_at".to_string(), Value::I64(expires_at)),
            ]));

            txn.put(storage_key, value_with_ttl)
        })?;
        Ok(Version::TxnId(commit_version))
    }

    /// Delete a key
    ///
    /// Returns `true` if the key existed and was deleted.
    pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool> {
        self.db.transaction(*run_id, |txn| {
            let storage_key = self.key_for(run_id, key);
            // Check if key exists before deleting
            let exists = txn.get(&storage_key)?.is_some();
            if exists {
                txn.delete(storage_key)?;
            }
            Ok(exists)
        })
    }

    /// Check if a key exists (FAST PATH)
    ///
    /// Uses direct snapshot read, bypassing transaction overhead.
    pub fn exists(&self, run_id: &RunId, key: &str) -> Result<bool> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let storage_key = self.key_for(run_id, key);

        Ok(snapshot.get(&storage_key)?.is_some())
    }

    // ========== Batch Operations (Fast Path) ==========

    /// Get multiple values in a single snapshot (FAST PATH) (M9: Returns versioned)
    ///
    /// Uses a single snapshot acquisition for all keys, ensuring:
    /// - Consistent point-in-time view across all keys
    /// - More efficient than multiple get() calls
    /// - No version mixing (all keys from same snapshot)
    ///
    /// # Performance
    /// For N keys: ~(snapshot_time + N * lookup_time)
    /// vs N * (snapshot_time + lookup_time) for individual gets
    ///
    /// # Returns
    /// Vec of Option<Versioned<Value>> in same order as input keys.
    /// None for keys that don't exist.
    ///
    /// # M9 Contract
    /// Returns `Versioned<Value>` with version information for each key.
    pub fn get_many(&self, run_id: &RunId, keys: &[&str]) -> Result<Vec<Option<Versioned<Value>>>> {
        use in_mem_core::traits::SnapshotView;

        // Single snapshot for consistency
        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);

        keys.iter()
            .map(|key| {
                let storage_key = Key::new_kv(ns.clone(), key);
                // M9: Return full Versioned<Value>
                Ok(snapshot.get(&storage_key)?)
            })
            .collect()
    }

    /// Get multiple values as a HashMap (FAST PATH) (M9: Returns versioned)
    ///
    /// Like get_many(), but returns a HashMap for easier lookup.
    /// Only includes keys that exist (missing keys are omitted).
    ///
    /// # Returns
    /// HashMap mapping key strings to their versioned values.
    /// Keys that don't exist are not included.
    ///
    /// # M9 Contract
    /// Returns `Versioned<Value>` with version information for each key.
    pub fn get_many_map(
        &self,
        run_id: &RunId,
        keys: &[&str],
    ) -> Result<std::collections::HashMap<String, Versioned<Value>>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);

        let mut result = std::collections::HashMap::with_capacity(keys.len());
        for key in keys {
            let storage_key = Key::new_kv(ns.clone(), *key);
            if let Some(vv) = snapshot.get(&storage_key)? {
                // M9: Return full Versioned<Value>
                result.insert((*key).to_string(), vv);
            }
        }
        Ok(result)
    }

    /// Check if a key exists (alias for exists, matches spec) (FAST PATH)
    pub fn contains(&self, run_id: &RunId, key: &str) -> Result<bool> {
        self.exists(run_id, key)
    }

    // ========== List Operations ==========

    /// List keys with optional prefix filter
    ///
    /// Returns all keys matching the prefix (or all keys if prefix is None).
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

    /// List key-value pairs with optional prefix filter (M9: Returns versioned)
    ///
    /// Returns all key-value pairs matching the prefix (or all if prefix is None).
    ///
    /// # M9 Contract
    /// Returns `Vec<(String, Versioned<Value>)>` with version information.
    pub fn list_with_values(
        &self,
        run_id: &RunId,
        prefix: Option<&str>,
    ) -> Result<Vec<(String, Versioned<Value>)>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let scan_prefix = Key::new_kv(ns, prefix.unwrap_or(""));

            let results = txn.scan_prefix(&scan_prefix)?;

            // M9: Return Versioned<Value> with version from scan
            Ok(results
                .into_iter()
                .filter_map(|(key, value)| {
                    key.user_key_string()
                        .map(|k| (k, Versioned::new(value, Version::TxnId(txn.start_version))))
                })
                .collect())
        })
    }

    // ========== Search API (M6) ==========

    /// Search KV entries
    ///
    /// Searches key names and string values using the simple scorer.
    /// Respects budget constraints (time and candidate limits).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use in_mem_primitives::KVStore;
    /// use in_mem_core::SearchRequest;
    ///
    /// let response = kv.search(&SearchRequest::new(run_id, "hello"))?;
    /// for hit in response.hits {
    ///     println!("Found: {:?} with score {}", hit.doc_ref, hit.score);
    /// }
    /// ```
    pub fn search(
        &self,
        req: &in_mem_core::SearchRequest,
    ) -> in_mem_core::error::Result<in_mem_core::SearchResponse> {
        use crate::searchable::{build_search_response, SearchCandidate};
        use in_mem_core::search_types::DocRef;
        use in_mem_core::traits::SnapshotView;
        use std::time::Instant;

        let start = Instant::now();
        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(&req.run_id);
        let scan_prefix = Key::new_kv(ns, "");

        let mut candidates = Vec::new();
        let mut truncated = false;

        // Scan all KV entries for this run
        for (key, versioned_value) in snapshot.scan_prefix(&scan_prefix)? {
            // Check budget constraints
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                truncated = true;
                break;
            }
            if candidates.len() >= req.budget.max_candidates_per_primitive {
                truncated = true;
                break;
            }

            // Time range filter
            if let Some((start_ts, end_ts)) = req.time_range {
                let ts = versioned_value.timestamp.as_micros();
                if ts < start_ts || ts > end_ts {
                    continue;
                }
            }

            // Extract searchable text
            let text = self.extract_search_text(&key, &versioned_value.value);

            // Extract user key as string for DocRef
            let user_key_str = String::from_utf8_lossy(&key.user_key).to_string();

            candidates.push(SearchCandidate::new(
                DocRef::Kv { run_id: req.run_id, key: user_key_str },
                text,
                Some(versioned_value.timestamp.as_micros()),
            ));
        }

        Ok(build_search_response(
            candidates,
            &req.query,
            req.k,
            truncated,
            start.elapsed().as_micros() as u64,
        ))
    }

    /// Extract searchable text from a KV entry
    fn extract_search_text(&self, key: &Key, value: &Value) -> String {
        let mut parts = Vec::new();

        // Include key name
        if let Some(key_str) = key.user_key_string() {
            parts.push(key_str);
        }

        // Include value based on type
        match value {
            Value::String(s) => parts.push(s.clone()),
            Value::Bytes(b) => {
                if let Ok(s) = std::str::from_utf8(b) {
                    parts.push(s.to_string());
                }
            }
            Value::I64(n) => parts.push(n.to_string()),
            Value::F64(n) => parts.push(n.to_string()),
            Value::Bool(b) => parts.push(b.to_string()),
            Value::Array(_) | Value::Map(_) => {
                if let Ok(s) = serde_json::to_string(value) {
                    parts.push(s);
                }
            }
            Value::Null => {}
        }

        parts.join(" ")
    }

    // ========== Multi-Operation API (Explicit Transactions) ==========

    /// Execute multiple KV operations atomically
    ///
    /// All operations within the closure are part of a single transaction.
    /// Either all succeed or all are rolled back.
    ///
    /// # Example
    ///
    /// ```ignore
    /// kv.transaction(&run_id, |txn| {
    ///     txn.put("key1", Value::I64(1))?;
    ///     txn.put("key2", Value::I64(2))?;
    ///     let val = txn.get("key1")?;
    ///     Ok(val)
    /// })?;
    /// ```
    pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut KVTransaction<'_>) -> Result<T>,
    {
        self.db.transaction(*run_id, |txn| {
            let mut kv_txn = KVTransaction {
                txn,
                run_id: *run_id,
            };
            f(&mut kv_txn)
        })
    }
}

/// Transaction handle for multi-key KV operations
///
/// Provides get/put/delete/list operations within an atomic transaction.
/// Changes are only visible after the transaction commits successfully.
pub struct KVTransaction<'a> {
    txn: &'a mut TransactionContext,
    run_id: RunId,
}

impl<'a> KVTransaction<'a> {
    /// Get a value within the transaction
    pub fn get(&mut self, key: &str) -> Result<Option<Value>> {
        let storage_key = Key::new_kv(Namespace::for_run(self.run_id), key);
        self.txn.get(&storage_key)
    }

    /// Put a value within the transaction
    pub fn put(&mut self, key: &str, value: Value) -> Result<()> {
        let storage_key = Key::new_kv(Namespace::for_run(self.run_id), key);
        self.txn.put(storage_key, value)
    }

    /// Delete a key within the transaction
    pub fn delete(&mut self, key: &str) -> Result<bool> {
        let storage_key = Key::new_kv(Namespace::for_run(self.run_id), key);
        let exists = self.txn.get(&storage_key)?.is_some();
        if exists {
            self.txn.delete(storage_key)?;
        }
        Ok(exists)
    }

    /// List keys within the transaction
    pub fn list(&mut self, prefix: Option<&str>) -> Result<Vec<String>> {
        let ns = Namespace::for_run(self.run_id);
        let scan_prefix = Key::new_kv(ns, prefix.unwrap_or(""));

        let results = self.txn.scan_prefix(&scan_prefix)?;

        Ok(results
            .into_iter()
            .filter_map(|(key, _)| key.user_key_string())
            .collect())
    }
}

// ========== Searchable Trait Implementation (M6) ==========

impl crate::searchable::Searchable for KVStore {
    fn search(
        &self,
        req: &in_mem_core::SearchRequest,
    ) -> in_mem_core::error::Result<in_mem_core::SearchResponse> {
        self.search(req)
    }

    fn primitive_kind(&self) -> in_mem_core::PrimitiveType {
        in_mem_core::PrimitiveType::Kv
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
    use in_mem_core::types::TypeTag;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<Database>, KVStore) {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path()).unwrap());
        let kv = KVStore::new(db.clone());
        (temp_dir, db, kv)
    }

    // ========== Core Structure Tests (Story #169) ==========

    #[test]
    fn test_kvstore_creation() {
        let (_temp, _db, kv) = setup();
        assert!(Arc::strong_count(kv.database()) >= 1);
    }

    #[test]
    fn test_kvstore_is_clone() {
        let (_temp, _db, kv1) = setup();
        let kv2 = kv1.clone();
        // Both point to same database
        assert!(Arc::ptr_eq(kv1.database(), kv2.database()));
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

    // ========== Single-Operation API Tests (Story #170) ==========

    #[test]
    fn test_put_and_get() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key1", Value::String("value1".into()))
            .unwrap();
        let result = kv.get(&run_id, "key1").unwrap();
        assert_eq!(result.map(|v| v.value), Some(Value::String("value1".into())));
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
        assert_eq!(result.map(|v| v.value), Some(Value::String("value2".into())));
    }

    #[test]
    fn test_delete() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key1", Value::String("value1".into()))
            .unwrap();
        assert!(kv.exists(&run_id, "key1").unwrap());

        let deleted = kv.delete(&run_id, "key1").unwrap();
        assert!(deleted);
        assert!(!kv.exists(&run_id, "key1").unwrap());
    }

    #[test]
    fn test_delete_nonexistent() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        let deleted = kv.delete(&run_id, "nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_exists() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        assert!(!kv.exists(&run_id, "key1").unwrap());
        kv.put(&run_id, "key1", Value::I64(42)).unwrap();
        assert!(kv.exists(&run_id, "key1").unwrap());
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
            kv.get(&run1, "shared-key").unwrap().map(|v| v.value),
            Some(Value::String("run1-value".into()))
        );
        assert_eq!(
            kv.get(&run2, "shared-key").unwrap().map(|v| v.value),
            Some(Value::String("run2-value".into()))
        );
    }

    #[test]
    fn test_put_with_ttl() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put_with_ttl(
            &run_id,
            "expiring-key",
            Value::String("temp".into()),
            Duration::from_secs(3600),
        )
        .unwrap();

        // Value is stored with metadata
        let result = kv.get(&run_id, "expiring-key").unwrap();
        assert!(result.is_some());

        // Verify the value is wrapped with TTL metadata
        if let Some(versioned) = result {
            if let Value::Map(map) = versioned.value {
                assert!(map.contains_key("value"));
                assert!(map.contains_key("expires_at"));
            } else {
                panic!("Expected Value::Map with TTL metadata");
            }
        }
    }

    // ========== Multi-Operation API Tests (Story #171) ==========

    #[test]
    fn test_multi_key_atomic() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.transaction(&run_id, |txn| {
            txn.put("key1", Value::String("val1".into()))?;
            txn.put("key2", Value::String("val2".into()))?;
            txn.put("key3", Value::String("val3".into()))?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            kv.get(&run_id, "key1").unwrap().map(|v| v.value),
            Some(Value::String("val1".into()))
        );
        assert_eq!(
            kv.get(&run_id, "key2").unwrap().map(|v| v.value),
            Some(Value::String("val2".into()))
        );
        assert_eq!(
            kv.get(&run_id, "key3").unwrap().map(|v| v.value),
            Some(Value::String("val3".into()))
        );
    }

    #[test]
    fn test_transaction_read_your_writes() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.transaction(&run_id, |txn| {
            txn.put("key", Value::I64(1))?;
            let val = txn.get("key")?;
            assert_eq!(val, Some(Value::I64(1)));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_transaction_delete() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        // Setup: create a key
        kv.put(&run_id, "key", Value::I64(1)).unwrap();

        // Delete in transaction
        kv.transaction(&run_id, |txn| {
            let deleted = txn.delete("key")?;
            assert!(deleted);
            // Read-your-deletes: should see None
            let val = txn.get("key")?;
            assert_eq!(val, None);
            Ok(())
        })
        .unwrap();

        // Verify deleted
        assert!(!kv.exists(&run_id, "key").unwrap());
    }

    // ========== List Operations Tests (Story #172) ==========

    #[test]
    fn test_list_all() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "a", Value::I64(1)).unwrap();
        kv.put(&run_id, "b", Value::I64(2)).unwrap();
        kv.put(&run_id, "c", Value::I64(3)).unwrap();

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

        kv.put(&run_id, "user:1", Value::I64(1)).unwrap();
        kv.put(&run_id, "user:2", Value::I64(2)).unwrap();
        kv.put(&run_id, "task:1", Value::I64(3)).unwrap();

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

        kv.put(&run_id, "key1", Value::I64(1)).unwrap();
        kv.put(&run_id, "key2", Value::I64(2)).unwrap();

        let keys = kv.list(&run_id, Some("nonexistent:")).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn test_list_with_values() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key1", Value::String("val1".into()))
            .unwrap();
        kv.put(&run_id, "key2", Value::String("val2".into()))
            .unwrap();

        let pairs = kv.list_with_values(&run_id, None).unwrap();
        assert_eq!(pairs.len(), 2);

        let pairs_map: std::collections::HashMap<_, _> = pairs.into_iter().collect();
        assert_eq!(pairs_map.get("key1").map(|v| &v.value), Some(&Value::String("val1".into())));
        assert_eq!(pairs_map.get("key2").map(|v| &v.value), Some(&Value::String("val2".into())));
    }

    #[test]
    fn test_list_run_isolation() {
        let (_temp, _db, kv) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        kv.put(&run1, "run1-key", Value::I64(1)).unwrap();
        kv.put(&run2, "run2-key", Value::I64(2)).unwrap();

        // Each run only sees its own keys
        let run1_keys = kv.list(&run1, None).unwrap();
        assert_eq!(run1_keys.len(), 1);
        assert!(run1_keys.contains(&"run1-key".to_string()));

        let run2_keys = kv.list(&run2, None).unwrap();
        assert_eq!(run2_keys.len(), 1);
        assert!(run2_keys.contains(&"run2-key".to_string()));
    }

    // ========== KVStoreExt Tests (Story #173) ==========

    #[test]
    fn test_kvstore_ext_in_transaction() {
        use crate::extensions::KVStoreExt;

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
        use crate::extensions::KVStoreExt;

        let (_temp, db, kv) = setup();
        let run_id = RunId::new();

        // Setup
        kv.put(&run_id, "key", Value::I64(42)).unwrap();

        // Delete via extension trait
        db.transaction(run_id, |txn| {
            txn.kv_delete("key")?;
            let val = txn.kv_get("key")?;
            assert_eq!(val, None);
            Ok(())
        })
        .unwrap();
    }

    // ========== Value Type Tests ==========

    #[test]
    fn test_various_value_types() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        // String
        kv.put(&run_id, "string", Value::String("hello".into()))
            .unwrap();
        assert_eq!(
            kv.get(&run_id, "string").unwrap().map(|v| v.value),
            Some(Value::String("hello".into()))
        );

        // Integer
        kv.put(&run_id, "int", Value::I64(42)).unwrap();
        assert_eq!(kv.get(&run_id, "int").unwrap().map(|v| v.value), Some(Value::I64(42)));

        // Float
        kv.put(&run_id, "float", Value::F64(3.14)).unwrap();
        assert_eq!(kv.get(&run_id, "float").unwrap().map(|v| v.value), Some(Value::F64(3.14)));

        // Boolean
        kv.put(&run_id, "bool", Value::Bool(true)).unwrap();
        assert_eq!(kv.get(&run_id, "bool").unwrap().map(|v| v.value), Some(Value::Bool(true)));

        // Null
        kv.put(&run_id, "null", Value::Null).unwrap();
        assert_eq!(kv.get(&run_id, "null").unwrap().map(|v| v.value), Some(Value::Null));

        // Bytes
        kv.put(&run_id, "bytes", Value::Bytes(vec![1, 2, 3]))
            .unwrap();
        assert_eq!(
            kv.get(&run_id, "bytes").unwrap().map(|v| v.value),
            Some(Value::Bytes(vec![1, 2, 3]))
        );

        // Array
        kv.put(
            &run_id,
            "array",
            Value::Array(vec![Value::I64(1), Value::I64(2)]),
        )
        .unwrap();
        // M9: Extract value from Versioned wrapper
        assert_eq!(
            kv.get(&run_id, "array").unwrap().map(|v| v.value),
            Some(Value::Array(vec![Value::I64(1), Value::I64(2)]))
        );
    }

    // ========== Fast Path Tests (Story #236) ==========

    #[test]
    fn test_fast_get_returns_correct_value() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key", Value::I64(42)).unwrap();

        let result = kv.get(&run_id, "key").unwrap();
        // M9: get() now returns Versioned<Value>
        assert_eq!(result.map(|v| v.value), Some(Value::I64(42)));
    }

    #[test]
    fn test_fast_get_returns_none_for_missing() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        let result = kv.get(&run_id, "missing").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_fast_get_equals_transaction_get() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key", Value::I64(42)).unwrap();

        let fast = kv.get(&run_id, "key").unwrap();
        let txn = kv.get_in_transaction(&run_id, "key").unwrap();

        // M9: Compare values extracted from Versioned wrappers
        assert_eq!(
            fast.map(|v| v.value),
            txn.map(|v| v.value),
            "Fast path must equal transaction read"
        );
    }

    #[test]
    fn test_fast_get_observational_equivalence() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        // Write some data
        kv.put(&run_id, "key1", Value::String("value1".into()))
            .unwrap();
        kv.put(&run_id, "key2", Value::I64(42)).unwrap();

        // Fast path reads
        let fast1 = kv.get(&run_id, "key1").unwrap();
        let fast2 = kv.get(&run_id, "key2").unwrap();
        let fast_missing = kv.get(&run_id, "missing").unwrap();

        // Transaction reads
        let txn1 = kv.get_in_transaction(&run_id, "key1").unwrap();
        let txn2 = kv.get_in_transaction(&run_id, "key2").unwrap();
        let txn_missing = kv.get_in_transaction(&run_id, "missing").unwrap();

        // M9: Compare values
        assert_eq!(fast1.map(|v| v.value), txn1.map(|v| v.value));
        assert_eq!(fast2.map(|v| v.value), txn2.map(|v| v.value));
        assert_eq!(fast_missing, txn_missing);
    }

    #[test]
    fn test_fast_exists_uses_fast_path() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        assert!(!kv.exists(&run_id, "key").unwrap());

        kv.put(&run_id, "key", Value::I64(42)).unwrap();

        assert!(kv.exists(&run_id, "key").unwrap());
    }

    #[test]
    fn test_fast_get_run_isolation() {
        let (_temp, _db, kv) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        kv.put(&run1, "shared-key", Value::String("run1-value".into()))
            .unwrap();
        kv.put(&run2, "shared-key", Value::String("run2-value".into()))
            .unwrap();

        // Fast path should respect run isolation
        // M9: Extract value from Versioned wrapper
        assert_eq!(
            kv.get(&run1, "shared-key").unwrap().map(|v| v.value),
            Some(Value::String("run1-value".into()))
        );
        assert_eq!(
            kv.get(&run2, "shared-key").unwrap().map(|v| v.value),
            Some(Value::String("run2-value".into()))
        );
    }

    // ========== Batch Get Tests (Story #237) ==========

    #[test]
    fn test_get_many_returns_all_values() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "a", Value::I64(1)).unwrap();
        kv.put(&run_id, "b", Value::I64(2)).unwrap();
        kv.put(&run_id, "c", Value::I64(3)).unwrap();

        let results = kv.get_many(&run_id, &["a", "b", "c", "missing"]).unwrap();

        // M9: get_many returns Versioned<Value>
        assert_eq!(results[0].as_ref().map(|v| &v.value), Some(&Value::I64(1)));
        assert_eq!(results[1].as_ref().map(|v| &v.value), Some(&Value::I64(2)));
        assert_eq!(results[2].as_ref().map(|v| &v.value), Some(&Value::I64(3)));
        assert_eq!(results[3], None);
    }

    #[test]
    fn test_get_many_preserves_order() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "z", Value::I64(26)).unwrap();
        kv.put(&run_id, "a", Value::I64(1)).unwrap();
        kv.put(&run_id, "m", Value::I64(13)).unwrap();

        // Order of results matches order of input keys
        let results = kv.get_many(&run_id, &["m", "z", "a"]).unwrap();

        // M9: get_many returns Versioned<Value>
        assert_eq!(results[0].as_ref().map(|v| &v.value), Some(&Value::I64(13))); // m
        assert_eq!(results[1].as_ref().map(|v| &v.value), Some(&Value::I64(26))); // z
        assert_eq!(results[2].as_ref().map(|v| &v.value), Some(&Value::I64(1))); // a
    }

    #[test]
    fn test_get_many_map() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key1", Value::String("val1".into()))
            .unwrap();
        kv.put(&run_id, "key2", Value::String("val2".into()))
            .unwrap();

        let map = kv
            .get_many_map(&run_id, &["key1", "key2", "missing"])
            .unwrap();

        assert_eq!(map.len(), 2); // missing is not included
        // M9: get_many_map returns Versioned<Value>
        assert_eq!(
            map.get("key1").map(|v| &v.value),
            Some(&Value::String("val1".into()))
        );
        assert_eq!(
            map.get("key2").map(|v| &v.value),
            Some(&Value::String("val2".into()))
        );
        assert_eq!(map.get("missing"), None);
    }

    #[test]
    fn test_get_many_empty_keys() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        let results = kv.get_many(&run_id, &[]).unwrap();
        assert!(results.is_empty());

        let map = kv.get_many_map(&run_id, &[]).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn test_contains() {
        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        assert!(!kv.contains(&run_id, "key").unwrap());

        kv.put(&run_id, "key", Value::I64(42)).unwrap();

        assert!(kv.contains(&run_id, "key").unwrap());
    }

    #[test]
    fn test_get_many_run_isolation() {
        let (_temp, _db, kv) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        kv.put(&run1, "key", Value::I64(1)).unwrap();
        kv.put(&run2, "key", Value::I64(2)).unwrap();

        let results1 = kv.get_many(&run1, &["key"]).unwrap();
        let results2 = kv.get_many(&run2, &["key"]).unwrap();

        // M9: get_many returns Versioned<Value>
        assert_eq!(results1[0].as_ref().map(|v| &v.value), Some(&Value::I64(1)));
        assert_eq!(results2[0].as_ref().map(|v| &v.value), Some(&Value::I64(2)));
    }

    // ========== Search API Tests (M6) ==========

    #[test]
    fn test_kv_search_basic() {
        use in_mem_core::SearchRequest;

        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "greeting", Value::String("hello world".into()))
            .unwrap();
        kv.put(&run_id, "farewell", Value::String("goodbye world".into()))
            .unwrap();
        kv.put(&run_id, "other", Value::String("something else".into()))
            .unwrap();

        let req = SearchRequest::new(run_id, "hello");
        let response = kv.search(&req).unwrap();

        assert_eq!(response.hits.len(), 1);
        assert_eq!(response.hits[0].rank, 1);
    }

    #[test]
    fn test_kv_search_by_key_name() {
        use in_mem_core::SearchRequest;

        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(
            &run_id,
            "user_email",
            Value::String("test@example.com".into()),
        )
        .unwrap();
        kv.put(&run_id, "user_name", Value::String("John".into()))
            .unwrap();

        // Search should match key names too
        let req = SearchRequest::new(run_id, "email");
        let response = kv.search(&req).unwrap();

        assert!(!response.hits.is_empty());
    }

    #[test]
    fn test_kv_search_respects_k() {
        use in_mem_core::SearchRequest;

        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        // Create many matching entries
        for i in 0..20 {
            kv.put(
                &run_id,
                &format!("key{}", i),
                Value::String(format!("hello document {}", i)),
            )
            .unwrap();
        }

        let req = SearchRequest::new(run_id, "hello").with_k(5);
        let response = kv.search(&req).unwrap();

        assert_eq!(response.hits.len(), 5);
    }

    #[test]
    fn test_kv_search_run_isolation() {
        use in_mem_core::SearchRequest;

        let (_temp, _db, kv) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        kv.put(&run1, "key", Value::String("hello from run1".into()))
            .unwrap();
        kv.put(&run2, "key", Value::String("hello from run2".into()))
            .unwrap();

        let req1 = SearchRequest::new(run1, "hello");
        let response1 = kv.search(&req1).unwrap();
        assert_eq!(response1.hits.len(), 1);

        let req2 = SearchRequest::new(run2, "hello");
        let response2 = kv.search(&req2).unwrap();
        assert_eq!(response2.hits.len(), 1);
    }

    #[test]
    fn test_kv_search_empty_results() {
        use in_mem_core::SearchRequest;

        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        kv.put(&run_id, "key", Value::String("hello world".into()))
            .unwrap();

        let req = SearchRequest::new(run_id, "nonexistent");
        let response = kv.search(&req).unwrap();

        assert!(response.hits.is_empty());
    }

    #[test]
    fn test_kv_searchable_trait() {
        use crate::searchable::Searchable;
        use in_mem_core::search_types::PrimitiveKind;
        use in_mem_core::SearchRequest;

        let (_temp, _db, kv) = setup();
        let run_id = RunId::new();

        // Test primitive_kind
        assert_eq!(kv.primitive_kind(), PrimitiveKind::Kv);

        // Test search via trait
        kv.put(&run_id, "key", Value::String("searchable test".into()))
            .unwrap();

        let req = SearchRequest::new(run_id, "searchable");
        let response = Searchable::search(&kv, &req).unwrap();

        assert!(!response.hits.is_empty());
    }
}
