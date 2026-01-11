//! UnifiedStore: MVP storage backend with BTreeMap and version management
//!
//! This module implements the Storage trait using:
//! - `BTreeMap<Key, VersionedValue>` for ordered key storage
//! - `parking_lot::RwLock` for thread-safe access
//! - `AtomicU64` for monotonically increasing version numbers
//!
//! # Design Notes
//!
//! - **No version history**: Each key stores only its latest value (acceptable for MVP)
//! - **Logical TTL expiration**: Expired values are filtered at read time, not deleted
//! - **Version allocation before write lock**: Prevents lock contention during version assignment

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;

use in_mem_core::{Key, Result, RunId, Storage, Value, VersionedValue};

/// Unified storage backend using BTreeMap with RwLock
///
/// Implements the Storage trait for MVP functionality.
/// Thread-safe through `parking_lot::RwLock` and `AtomicU64`.
#[derive(Debug)]
pub struct UnifiedStore {
    /// The main data store: ordered map from Key to VersionedValue
    data: Arc<RwLock<BTreeMap<Key, VersionedValue>>>,
    /// Global version counter for monotonically increasing versions
    version: AtomicU64,
}

impl UnifiedStore {
    /// Create a new empty UnifiedStore
    ///
    /// Initial version is 0 (no writes have occurred).
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(BTreeMap::new())),
            version: AtomicU64::new(0),
        }
    }

    /// Allocate the next version atomically
    ///
    /// Uses fetch_add with SeqCst ordering to ensure:
    /// - Versions are unique across all threads
    /// - Versions are monotonically increasing (1, 2, 3, ...)
    fn next_version(&self) -> u64 {
        self.version.fetch_add(1, Ordering::SeqCst) + 1
    }
}

impl Default for UnifiedStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage for UnifiedStore {
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>> {
        let data = self.data.read();
        match data.get(key) {
            Some(vv) if !vv.is_expired() => Ok(Some(vv.clone())),
            _ => Ok(None),
        }
    }

    fn get_versioned(&self, key: &Key, max_version: u64) -> Result<Option<VersionedValue>> {
        let data = self.data.read();
        match data.get(key) {
            Some(vv) if vv.version <= max_version && !vv.is_expired() => Ok(Some(vv.clone())),
            _ => Ok(None),
        }
    }

    fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> Result<u64> {
        // Allocate version BEFORE acquiring write lock
        let version = self.next_version();

        let versioned_value = VersionedValue::new(value, version, ttl);

        let mut data = self.data.write();
        data.insert(key, versioned_value);

        Ok(version)
    }

    fn delete(&self, key: &Key) -> Result<Option<VersionedValue>> {
        let mut data = self.data.write();
        Ok(data.remove(key))
    }

    fn scan_prefix(&self, prefix: &Key, max_version: u64) -> Result<Vec<(Key, VersionedValue)>> {
        let data = self.data.read();

        let results: Vec<(Key, VersionedValue)> = data
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .filter(|(_, vv)| vv.version <= max_version && !vv.is_expired())
            .map(|(k, vv)| (k.clone(), vv.clone()))
            .collect();

        Ok(results)
    }

    fn scan_by_run(&self, run_id: RunId, max_version: u64) -> Result<Vec<(Key, VersionedValue)>> {
        let data = self.data.read();

        let results: Vec<(Key, VersionedValue)> = data
            .iter()
            .filter(|(k, vv)| {
                k.namespace.run_id == run_id && vv.version <= max_version && !vv.is_expired()
            })
            .map(|(k, vv)| (k.clone(), vv.clone()))
            .collect();

        Ok(results)
    }

    fn current_version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use in_mem_core::{Namespace, TypeTag};
    use std::thread;

    /// Helper to create a test namespace
    fn test_namespace() -> Namespace {
        let run_id = RunId::new();
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        )
    }

    /// Helper to create a test key
    fn test_key(ns: &Namespace, user_key: &str) -> Key {
        Key::new_kv(ns.clone(), user_key)
    }

    // ========================================
    // Test 1: Store Creation
    // ========================================

    #[test]
    fn test_store_creation() {
        let store = UnifiedStore::new();

        // Empty store should have current_version = 0
        assert_eq!(store.current_version(), 0);

        // Get on empty store should return None
        let ns = test_namespace();
        let key = test_key(&ns, "nonexistent");
        let result = store.get(&key).unwrap();
        assert!(result.is_none());
    }

    // ========================================
    // Test 2: Put and Get
    // ========================================

    #[test]
    fn test_put_and_get() {
        let store = UnifiedStore::new();
        let ns = test_namespace();
        let key = test_key(&ns, "mykey");
        let value = Value::String("hello".to_string());

        // Put should return version 1
        let version = store.put(key.clone(), value.clone(), None).unwrap();
        assert_eq!(version, 1);

        // Get should return the value
        let result = store.get(&key).unwrap();
        assert!(result.is_some());

        let vv = result.unwrap();
        assert_eq!(vv.value, value);
        assert_eq!(vv.version, 1);
        assert!(vv.ttl.is_none());
    }

    // ========================================
    // Test 3: Version Monotonicity
    // ========================================

    #[test]
    fn test_version_monotonicity() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        // Multiple puts should get versions 1, 2, 3, ...
        let key1 = test_key(&ns, "key1");
        let key2 = test_key(&ns, "key2");
        let key3 = test_key(&ns, "key3");

        let v1 = store.put(key1, Value::I64(1), None).unwrap();
        let v2 = store.put(key2, Value::I64(2), None).unwrap();
        let v3 = store.put(key3, Value::I64(3), None).unwrap();

        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
        assert_eq!(v3, 3);

        // current_version should reflect the last assigned version
        assert_eq!(store.current_version(), 3);
    }

    // ========================================
    // Test 4: Get Versioned
    // ========================================

    #[test]
    fn test_get_versioned() {
        let store = UnifiedStore::new();
        let ns = test_namespace();
        let key = test_key(&ns, "versioned_key");

        // Write value at version 1
        let value1 = Value::String("v1".to_string());
        store.put(key.clone(), value1.clone(), None).unwrap();

        // Overwrite with new value at version 2
        let value2 = Value::String("v2".to_string());
        store.put(key.clone(), value2.clone(), None).unwrap();

        // get_versioned with max_version=1 should return None
        // (because the current value is at version 2)
        let result = store.get_versioned(&key, 1).unwrap();
        assert!(result.is_none());

        // get_versioned with max_version=2 should return value2
        let result = store.get_versioned(&key, 2).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, value2);

        // get_versioned with max_version=100 should also return value2
        let result = store.get_versioned(&key, 100).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, value2);
    }

    // ========================================
    // Test 5: Delete
    // ========================================

    #[test]
    fn test_delete() {
        let store = UnifiedStore::new();
        let ns = test_namespace();
        let key = test_key(&ns, "delete_me");
        let value = Value::Bool(true);

        // Put then delete
        store.put(key.clone(), value.clone(), None).unwrap();

        let deleted = store.delete(&key).unwrap();
        assert!(deleted.is_some());
        assert_eq!(deleted.unwrap().value, value);

        // Get should return None after delete
        let result = store.get(&key).unwrap();
        assert!(result.is_none());

        // Delete non-existent key should return None
        let result = store.delete(&key).unwrap();
        assert!(result.is_none());
    }

    // ========================================
    // Test 6: TTL Expiration
    // ========================================

    #[test]
    fn test_ttl_expiration() {
        let store = UnifiedStore::new();
        let ns = test_namespace();
        let key = test_key(&ns, "ephemeral");
        let value = Value::String("temporary".to_string());

        // Put with 1 second TTL
        let ttl = Duration::from_secs(1);
        store.put(key.clone(), value.clone(), Some(ttl)).unwrap();

        // Should be readable immediately
        let result = store.get(&key).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, value);

        // After expiration, should return None
        // We simulate expiration by waiting (for real test)
        // For now, we test the is_expired logic separately

        // Create an expired value manually by modifying timestamp
        let mut vv = VersionedValue::new(value.clone(), 100, Some(Duration::from_secs(1)));
        vv.timestamp -= 2; // Set timestamp to 2 seconds ago
        assert!(vv.is_expired());
    }

    // ========================================
    // Test 7: Scan Prefix
    // ========================================

    #[test]
    fn test_scan_prefix() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        // Insert keys with different prefixes
        store
            .put(test_key(&ns, "user:alice"), Value::I64(1), None)
            .unwrap();
        store
            .put(test_key(&ns, "user:bob"), Value::I64(2), None)
            .unwrap();
        store
            .put(test_key(&ns, "user:charlie"), Value::I64(3), None)
            .unwrap();
        store
            .put(test_key(&ns, "config:db"), Value::I64(100), None)
            .unwrap();

        // Scan for "user:" prefix
        let prefix = test_key(&ns, "user:");
        let results = store.scan_prefix(&prefix, u64::MAX).unwrap();

        assert_eq!(results.len(), 3);

        // Results should be sorted by user_key
        let keys: Vec<String> = results
            .iter()
            .map(|(k, _)| String::from_utf8_lossy(&k.user_key).to_string())
            .collect();
        assert_eq!(keys, vec!["user:alice", "user:bob", "user:charlie"]);
    }

    // ========================================
    // Test 8: Scan by Run
    // ========================================

    #[test]
    fn test_scan_by_run() {
        let store = UnifiedStore::new();

        // Create two different runs
        let run1 = RunId::new();
        let run2 = RunId::new();

        let ns1 = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run1,
        );
        let ns2 = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run2,
        );

        // Insert keys for both runs
        store
            .put(Key::new_kv(ns1.clone(), "key1"), Value::I64(1), None)
            .unwrap();
        store
            .put(Key::new_kv(ns1.clone(), "key2"), Value::I64(2), None)
            .unwrap();
        store
            .put(Key::new_kv(ns2.clone(), "key3"), Value::I64(3), None)
            .unwrap();

        // Scan for run1
        let results = store.scan_by_run(run1, u64::MAX).unwrap();
        assert_eq!(results.len(), 2);

        // Scan for run2
        let results = store.scan_by_run(run2, u64::MAX).unwrap();
        assert_eq!(results.len(), 1);
    }

    // ========================================
    // Test 9: Concurrent Writes
    // ========================================

    #[test]
    fn test_concurrent_writes() {
        let store = Arc::new(UnifiedStore::new());
        let num_threads = 10;
        let writes_per_thread = 100;

        let mut handles = vec![];

        for thread_id in 0..num_threads {
            let store = Arc::clone(&store);

            let handle = thread::spawn(move || {
                let ns = Namespace::new(
                    "tenant".to_string(),
                    "app".to_string(),
                    "agent".to_string(),
                    RunId::new(),
                );

                for i in 0..writes_per_thread {
                    let key = Key::new_kv(ns.clone(), format!("t{}:k{}", thread_id, i));
                    let value = Value::I64((thread_id * writes_per_thread + i) as i64);
                    store.put(key, value, None).unwrap();
                }
            });

            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Total writes = 10 threads × 100 writes = 1000
        let expected_version = num_threads * writes_per_thread;
        assert_eq!(store.current_version(), expected_version as u64);
    }

    // ========================================
    // Additional Tests
    // ========================================

    #[test]
    fn test_overwrite_updates_version() {
        let store = UnifiedStore::new();
        let ns = test_namespace();
        let key = test_key(&ns, "overwrite_test");

        let v1 = store.put(key.clone(), Value::I64(1), None).unwrap();
        let v2 = store.put(key.clone(), Value::I64(2), None).unwrap();

        assert_eq!(v1, 1);
        assert_eq!(v2, 2);

        // The stored value should have version 2
        let result = store.get(&key).unwrap().unwrap();
        assert_eq!(result.version, 2);
        assert_eq!(result.value, Value::I64(2));
    }

    #[test]
    fn test_scan_prefix_respects_max_version() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        // Insert at version 1
        store
            .put(test_key(&ns, "key1"), Value::I64(1), None)
            .unwrap();

        // Insert at version 2
        store
            .put(test_key(&ns, "key2"), Value::I64(2), None)
            .unwrap();

        // Scan with max_version=1 should only return key1
        let prefix = test_key(&ns, "key");
        let results = store.scan_prefix(&prefix, 1).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(String::from_utf8_lossy(&results[0].0.user_key), "key1");
    }

    #[test]
    fn test_scan_by_run_respects_max_version() {
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Insert at version 1
        store
            .put(Key::new_kv(ns.clone(), "key1"), Value::I64(1), None)
            .unwrap();

        // Insert at version 2
        store
            .put(Key::new_kv(ns.clone(), "key2"), Value::I64(2), None)
            .unwrap();

        // Scan with max_version=1 should only return key1
        let results = store.scan_by_run(run_id, 1).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_different_type_tags_not_in_prefix_scan() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        // Insert KV key
        let kv_key = Key::new_kv(ns.clone(), "data");
        store.put(kv_key.clone(), Value::I64(1), None).unwrap();

        // Insert Event key (different TypeTag)
        let event_key = Key::new_event(ns.clone(), 1);
        store.put(event_key, Value::I64(2), None).unwrap();

        // Scan with KV prefix should only return KV key
        let prefix = Key::new_kv(ns.clone(), "");
        let results = store.scan_prefix(&prefix, u64::MAX).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.type_tag, TypeTag::KV);
    }

    #[test]
    fn test_store_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<UnifiedStore>();
        assert_sync::<UnifiedStore>();
    }

    #[test]
    fn test_default_trait() {
        let store = UnifiedStore::default();
        assert_eq!(store.current_version(), 0);
    }
}
