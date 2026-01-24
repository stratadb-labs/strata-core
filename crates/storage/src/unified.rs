//! UnifiedStore: MVP storage backend with BTreeMap and version management
//!
//! This module implements the Storage trait using:
//! - `BTreeMap<Key, StoredValue>` for ordered key storage with TTL
//! - `parking_lot::RwLock` for thread-safe access
//! - `AtomicU64` for monotonically increasing version numbers
//! - Secondary indices for efficient run and type queries
//!
//! # Design Notes
//!
//! - **No version history**: Each key stores only its latest value (acceptable for MVP)
//! - **Logical TTL expiration**: Expired values are filtered at read time, not deleted
//! - **Version allocation before write lock**: Prevents lock contention during version assignment
//! - **Secondary indices**: run_index and type_index enable O(subset) queries instead of O(total)
//!
//! # Storage vs Contract Types
//!
//! - `StoredValue`: Internal storage type that includes TTL (storage concern)
//! - `VersionedValue`: Contract type returned to callers (no TTL)

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;

use strata_core::{Key, Result, RunId, Storage, Timestamp, TypeTag, Value, Version, VersionedValue};

use crate::index::{RunIndex, TypeIndex};
use crate::snapshot::ClonedSnapshotView;
use crate::stored_value::StoredValue;
use crate::ttl::TTLIndex;

/// Unified storage backend using BTreeMap with RwLock
///
/// Implements the Storage trait for MVP functionality.
/// Thread-safe through `parking_lot::RwLock` and `AtomicU64`.
///
/// # Secondary Indices
///
/// - `run_index`: Maps RunId → Set<Key> for efficient run-scoped queries (O(run size) vs O(total))
/// - `type_index`: Maps TypeTag → Set<Key> for efficient type-scoped queries
/// - `ttl_index`: Maps expiry_timestamp → Set<Key> for efficient TTL cleanup
///
/// All indices are updated atomically with the main data store within the same write lock.
#[derive(Debug)]
pub struct UnifiedStore {
    /// The main data store: ordered map from Key to StoredValue (includes TTL)
    data: Arc<RwLock<BTreeMap<Key, StoredValue>>>,
    /// Secondary index: RunId → Keys for efficient run-scoped queries
    run_index: Arc<RwLock<RunIndex>>,
    /// Secondary index: TypeTag → Keys for efficient type-scoped queries
    type_index: Arc<RwLock<TypeIndex>>,
    /// TTL index: expiry_timestamp → Keys for efficient cleanup
    ttl_index: Arc<RwLock<TTLIndex>>,
    /// Global version counter for monotonically increasing versions
    version: AtomicU64,
}

impl UnifiedStore {
    /// Create a new empty UnifiedStore
    ///
    /// Initializes the main data store and secondary indices.
    /// Initial version is 0 (no writes have occurred).
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(BTreeMap::new())),
            run_index: Arc::new(RwLock::new(RunIndex::new())),
            type_index: Arc::new(RwLock::new(TypeIndex::new())),
            ttl_index: Arc::new(RwLock::new(TTLIndex::new())),
            version: AtomicU64::new(0),
        }
    }

    /// Calculate expiry timestamp from a StoredValue
    ///
    /// Returns Some(timestamp) if the value has a TTL, None otherwise.
    fn expiry_timestamp(sv: &StoredValue) -> Option<Timestamp> {
        sv.expiry_timestamp()
    }

    /// Find all keys that have expired before the current time
    ///
    /// Uses ttl_index for efficient O(expired count) lookup instead of O(total data).
    /// Returns keys that should be cleaned up by the TTL cleaner.
    pub fn find_expired_keys(&self) -> Result<Vec<Key>> {
        let now = Timestamp::now();
        let ttl_idx = self.ttl_index.read();
        Ok(ttl_idx.find_expired(now))
    }

    /// Scan all keys of a given type at or before max_version
    ///
    /// Uses type_index for efficient O(type size) lookup instead of O(total data).
    /// Returns all key-value pairs where:
    /// - Key has the specified type_tag
    /// - Value version <= max_version
    /// - Value is not expired
    pub fn scan_by_type(
        &self,
        type_tag: TypeTag,
        max_version: u64,
    ) -> Result<Vec<(Key, VersionedValue)>> {
        let type_idx = self.type_index.read();
        let data = self.data.read();

        let results = if let Some(keys) = type_idx.get(&type_tag) {
            keys.iter()
                .filter_map(|key| {
                    data.get(key).and_then(|sv| {
                        if sv.version().as_u64() <= max_version && !sv.is_expired() {
                            Some((key.clone(), sv.versioned().clone()))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(results)
    }

    /// Allocate the next version atomically
    ///
    /// Uses fetch_add with SeqCst ordering to ensure:
    /// - Versions are unique across all threads
    /// - Versions are monotonically increasing (1, 2, 3, ...)
    fn next_version(&self) -> u64 {
        self.version.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Create a snapshot of the current state
    ///
    /// This is the MVP implementation - creates a deep clone of the BTreeMap.
    /// The snapshot captures the data at the current version and is immutable.
    ///
    /// # Performance
    ///
    /// This operation is O(n) where n is the number of keys in the store.
    /// It clones the entire BTreeMap, which is expensive but correct for MVP.
    ///
    /// # Future Optimization
    ///
    /// The `SnapshotView` trait abstraction allows replacing this with a lazy
    /// implementation (LazySnapshotView) that reads from live storage with
    /// version filtering, avoiding the clone cost.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let store = UnifiedStore::new();
    /// store.put(key, value, None);
    ///
    /// let snapshot = store.create_snapshot();
    ///
    /// // Writes after snapshot creation are not visible in snapshot
    /// store.put(key2, value2, None);
    /// assert!(snapshot.get(&key2).unwrap().is_none());
    /// ```
    pub fn create_snapshot(&self) -> ClonedSnapshotView {
        // IMPORTANT: Acquire read lock BEFORE reading version to prevent race condition.
        // If we read version first, another thread could:
        // 1. Complete a write with version N
        // 2. Call fetch_max(N) to update storage version
        // 3. Release write lock
        // Meanwhile we'd have read version N-1 but then clone data with version N,
        // causing snapshot reads to incorrectly return None (N > N-1).
        let data = self.data.read();
        let version = self.current_version();
        ClonedSnapshotView::new(version, data.clone())
    }

    /// Apply a batch of writes and deletes atomically
    ///
    /// This method holds the write lock during ALL operations, ensuring that
    /// no snapshot can see a partial transaction. This is critical for
    /// transaction atomicity.
    ///
    /// # Arguments
    /// * `writes` - Vector of (key, value) pairs to write
    /// * `deletes` - Vector of keys to delete
    /// * `version` - The version to assign to all operations
    ///
    /// # Atomicity
    ///
    /// All writes and deletes are applied under a single write lock acquisition.
    /// Other threads cannot see intermediate states.
    pub fn apply_batch(
        &self,
        writes: &[(Key, Value)],
        deletes: &[Key],
        version: u64,
    ) -> Result<()> {
        // Acquire ALL locks ONCE for the entire batch
        let mut data = self.data.write();
        let mut run_idx = self.run_index.write();
        let mut type_idx = self.type_index.write();
        let mut ttl_idx = self.ttl_index.write();

        // Apply all writes
        for (key, value) in writes {
            let stored_value = StoredValue::new(value.clone(), Version::txn(version), None);

            // Check if key already exists with TTL (need to remove old TTL entry)
            if let Some(old_value) = data.get(key) {
                if let Some(old_expiry) = Self::expiry_timestamp(old_value) {
                    ttl_idx.remove(old_expiry, key);
                }
            }

            // Insert into main storage
            data.insert(key.clone(), stored_value);

            // Update secondary indices
            run_idx.insert(key.namespace.run_id, key.clone());
            type_idx.insert(key.type_tag, key.clone());
        }

        // Apply all deletes
        for key in deletes {
            let removed = data.remove(key);

            if let Some(ref value) = removed {
                // Update secondary indices
                run_idx.remove(key.namespace.run_id, key);
                type_idx.remove(key.type_tag, key);

                // Remove from TTL index if it had TTL
                if let Some(expiry) = Self::expiry_timestamp(value) {
                    ttl_idx.remove(expiry, key);
                }
            }
        }

        // Update global version to be at least this version
        // This ensures current_version() reflects the max version in the store
        self.version
            .fetch_max(version, std::sync::atomic::Ordering::SeqCst);

        Ok(())
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
            Some(sv) if !sv.is_expired() => Ok(Some(sv.versioned().clone())),
            _ => Ok(None),
        }
    }

    fn get_versioned(&self, key: &Key, max_version: u64) -> Result<Option<VersionedValue>> {
        let data = self.data.read();
        match data.get(key) {
            Some(sv) if sv.version().as_u64() <= max_version && !sv.is_expired() => {
                Ok(Some(sv.versioned().clone()))
            }
            _ => Ok(None),
        }
    }

    fn get_history(
        &self,
        key: &Key,
        limit: Option<usize>,
        before_version: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        // UnifiedStore only keeps one version per key (no MVCC)
        // Return current version if it exists and matches constraints
        let data = self.data.read();
        match data.get(key) {
            Some(sv) if !sv.is_expired() => {
                // Check before_version constraint
                if let Some(before) = before_version {
                    if sv.version().as_u64() >= before {
                        return Ok(Vec::new());
                    }
                }
                // Check limit (if limit is 0, return empty)
                if limit == Some(0) {
                    return Ok(Vec::new());
                }
                Ok(vec![sv.versioned().clone()])
            }
            _ => Ok(Vec::new()),
        }
    }

    fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> Result<u64> {
        // Allocate version BEFORE acquiring write lock
        let version = self.next_version();

        let stored_value = StoredValue::new(value, Version::txn(version), ttl);
        let new_expiry = Self::expiry_timestamp(&stored_value);

        // Acquire ALL locks (data + indices) for atomic update
        let mut data = self.data.write();
        let mut run_idx = self.run_index.write();
        let mut type_idx = self.type_index.write();
        let mut ttl_idx = self.ttl_index.write();

        // Check if key already exists with TTL (need to remove old TTL entry)
        if let Some(old_value) = data.get(&key) {
            if let Some(old_expiry) = Self::expiry_timestamp(old_value) {
                ttl_idx.remove(old_expiry, &key);
            }
        }

        // Insert into main storage
        data.insert(key.clone(), stored_value);

        // Update secondary indices
        run_idx.insert(key.namespace.run_id, key.clone());
        type_idx.insert(key.type_tag, key.clone());

        // Update TTL index if TTL is set
        if let Some(expiry) = new_expiry {
            ttl_idx.insert(expiry, key);
        }

        Ok(version)
    }

    fn delete(&self, key: &Key) -> Result<Option<VersionedValue>> {
        // Acquire ALL locks for atomic update
        let mut data = self.data.write();
        let mut run_idx = self.run_index.write();
        let mut type_idx = self.type_index.write();
        let mut ttl_idx = self.ttl_index.write();

        let removed = data.remove(key);

        if let Some(ref sv) = removed {
            // Update secondary indices
            run_idx.remove(key.namespace.run_id, key);
            type_idx.remove(key.type_tag, key);

            // Remove from TTL index if it had TTL
            if let Some(expiry) = Self::expiry_timestamp(sv) {
                ttl_idx.remove(expiry, key);
            }
        }

        Ok(removed.map(|sv| sv.into_versioned()))
    }

    fn scan_prefix(&self, prefix: &Key, max_version: u64) -> Result<Vec<(Key, VersionedValue)>> {
        let data = self.data.read();

        let results: Vec<(Key, VersionedValue)> = data
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .filter(|(_, sv)| sv.version().as_u64() <= max_version && !sv.is_expired())
            .map(|(k, sv)| (k.clone(), sv.versioned().clone()))
            .collect();

        Ok(results)
    }

    fn scan_by_run(&self, run_id: RunId, max_version: u64) -> Result<Vec<(Key, VersionedValue)>> {
        // Use run_index for efficient O(run size) lookup instead of O(total data)
        let run_idx = self.run_index.read();
        let data = self.data.read();

        let results = if let Some(keys) = run_idx.get(&run_id) {
            keys.iter()
                .filter_map(|key| {
                    data.get(key).and_then(|sv| {
                        if sv.version().as_u64() <= max_version && !sv.is_expired() {
                            Some((key.clone(), sv.versioned().clone()))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        } else {
            Vec::new()
        };

        Ok(results)
    }

    fn current_version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    fn put_with_version(
        &self,
        key: Key,
        value: Value,
        version: u64,
        ttl: Option<Duration>,
    ) -> Result<()> {
        let stored_value = StoredValue::new(value, Version::txn(version), ttl);
        let new_expiry = Self::expiry_timestamp(&stored_value);

        // Acquire ALL locks (data + indices) for atomic update
        let mut data = self.data.write();
        let mut run_idx = self.run_index.write();
        let mut type_idx = self.type_index.write();
        let mut ttl_idx = self.ttl_index.write();

        // Check if key already exists with TTL (need to remove old TTL entry)
        if let Some(old_value) = data.get(&key) {
            if let Some(old_expiry) = Self::expiry_timestamp(old_value) {
                ttl_idx.remove(old_expiry, &key);
            }
        }

        // Insert into main storage
        data.insert(key.clone(), stored_value);

        // Update secondary indices
        run_idx.insert(key.namespace.run_id, key.clone());
        type_idx.insert(key.type_tag, key.clone());

        // Update TTL index if TTL is set
        if let Some(expiry) = new_expiry {
            ttl_idx.insert(expiry, key);
        }

        // Update global version to be at least this version
        // This ensures current_version() reflects the max version in the store
        self.version
            .fetch_max(version, std::sync::atomic::Ordering::SeqCst);

        Ok(())
    }

    fn delete_with_version(&self, key: &Key, version: u64) -> Result<Option<VersionedValue>> {
        // Acquire ALL locks for atomic update
        let mut data = self.data.write();
        let mut run_idx = self.run_index.write();
        let mut type_idx = self.type_index.write();
        let mut ttl_idx = self.ttl_index.write();

        let removed = data.remove(key);

        if let Some(ref sv) = removed {
            // Update secondary indices
            run_idx.remove(key.namespace.run_id, key);
            type_idx.remove(key.type_tag, key);

            // Remove from TTL index if it had TTL
            if let Some(expiry) = Self::expiry_timestamp(sv) {
                ttl_idx.remove(expiry, key);
            }
        }

        // Update global version to be at least this version
        self.version
            .fetch_max(version, std::sync::atomic::Ordering::SeqCst);

        Ok(removed.map(|sv| sv.into_versioned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{Namespace, TypeTag};
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
        assert_eq!(vv.version, Version::txn(1));
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

        let v1 = store.put(key1, Value::Int(1), None).unwrap();
        let v2 = store.put(key2, Value::Int(2), None).unwrap();
        let v3 = store.put(key3, Value::Int(3), None).unwrap();

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
        // For now, we test the is_expired logic separately using StoredValue

        // Create an expired value manually using old timestamp
        let old_ts = Timestamp::from_micros(0); // Epoch = ancient past
        let sv = StoredValue::with_timestamp(
            value.clone(),
            Version::txn(100),
            old_ts,
            Some(Duration::from_secs(1)),
        );
        assert!(sv.is_expired());
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
            .put(test_key(&ns, "user:alice"), Value::Int(1), None)
            .unwrap();
        store
            .put(test_key(&ns, "user:bob"), Value::Int(2), None)
            .unwrap();
        store
            .put(test_key(&ns, "user:charlie"), Value::Int(3), None)
            .unwrap();
        store
            .put(test_key(&ns, "config:db"), Value::Int(100), None)
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
            .put(Key::new_kv(ns1.clone(), "key1"), Value::Int(1), None)
            .unwrap();
        store
            .put(Key::new_kv(ns1.clone(), "key2"), Value::Int(2), None)
            .unwrap();
        store
            .put(Key::new_kv(ns2.clone(), "key3"), Value::Int(3), None)
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
                    let value = Value::Int((thread_id * writes_per_thread + i) as i64);
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

        let v1 = store.put(key.clone(), Value::Int(1), None).unwrap();
        let v2 = store.put(key.clone(), Value::Int(2), None).unwrap();

        assert_eq!(v1, 1);
        assert_eq!(v2, 2);

        // The stored value should have version 2
        let result = store.get(&key).unwrap().unwrap();
        assert_eq!(result.version, Version::txn(2));
        assert_eq!(result.value, Value::Int(2));
    }

    #[test]
    fn test_scan_prefix_respects_max_version() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        // Insert at version 1
        store
            .put(test_key(&ns, "key1"), Value::Int(1), None)
            .unwrap();

        // Insert at version 2
        store
            .put(test_key(&ns, "key2"), Value::Int(2), None)
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
            .put(Key::new_kv(ns.clone(), "key1"), Value::Int(1), None)
            .unwrap();

        // Insert at version 2
        store
            .put(Key::new_kv(ns.clone(), "key2"), Value::Int(2), None)
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
        store.put(kv_key.clone(), Value::Int(1), None).unwrap();

        // Insert Event key (different TypeTag)
        let event_key = Key::new_event(ns.clone(), 1);
        store.put(event_key, Value::Int(2), None).unwrap();

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

    // ========================================
    // Secondary Index Tests (Story #13)
    // ========================================

    #[test]
    fn test_scan_by_run_uses_index() {
        // This test verifies that scan_by_run uses the run_index efficiently.
        // It creates multiple runs and ensures only the requested run's keys are returned.
        let store = UnifiedStore::new();

        // Create three different runs
        let run1 = RunId::new();
        let run2 = RunId::new();
        let run3 = RunId::new();

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
        let ns3 = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run3,
        );

        // Insert keys for all three runs
        store
            .put(Key::new_kv(ns1.clone(), "key1"), Value::Int(1), None)
            .unwrap();
        store
            .put(Key::new_kv(ns1.clone(), "key2"), Value::Int(2), None)
            .unwrap();
        store
            .put(Key::new_kv(ns2.clone(), "key3"), Value::Int(3), None)
            .unwrap();
        store
            .put(Key::new_kv(ns3.clone(), "key4"), Value::Int(4), None)
            .unwrap();
        store
            .put(Key::new_kv(ns3.clone(), "key5"), Value::Int(5), None)
            .unwrap();
        store
            .put(Key::new_kv(ns3.clone(), "key6"), Value::Int(6), None)
            .unwrap();

        // Scan for run1 - should get exactly 2 keys
        let results1 = store.scan_by_run(run1, u64::MAX).unwrap();
        assert_eq!(results1.len(), 2);

        // Scan for run2 - should get exactly 1 key
        let results2 = store.scan_by_run(run2, u64::MAX).unwrap();
        assert_eq!(results2.len(), 1);

        // Scan for run3 - should get exactly 3 keys
        let results3 = store.scan_by_run(run3, u64::MAX).unwrap();
        assert_eq!(results3.len(), 3);

        // Scan for non-existent run - should get 0 keys
        let results_empty = store.scan_by_run(RunId::new(), u64::MAX).unwrap();
        assert_eq!(results_empty.len(), 0);
    }

    #[test]
    fn test_scan_by_type() {
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Insert different types of keys
        store
            .put(Key::new_kv(ns.clone(), "kv1"), Value::Int(1), None)
            .unwrap();
        store
            .put(Key::new_kv(ns.clone(), "kv2"), Value::Int(2), None)
            .unwrap();
        store
            .put(Key::new_event(ns.clone(), 1), Value::Int(100), None)
            .unwrap();
        store
            .put(Key::new_event(ns.clone(), 2), Value::Int(101), None)
            .unwrap();
        store
            .put(Key::new_event(ns.clone(), 3), Value::Int(102), None)
            .unwrap();
        store
            .put(Key::new_state(ns.clone(), "test-state"), Value::Int(200), None)
            .unwrap();

        // Scan by KV type - should get 2 keys
        let kv_results = store.scan_by_type(TypeTag::KV, u64::MAX).unwrap();
        assert_eq!(kv_results.len(), 2);
        for (key, _) in &kv_results {
            assert_eq!(key.type_tag, TypeTag::KV);
        }

        // Scan by Event type - should get 3 keys
        let event_results = store.scan_by_type(TypeTag::Event, u64::MAX).unwrap();
        assert_eq!(event_results.len(), 3);
        for (key, _) in &event_results {
            assert_eq!(key.type_tag, TypeTag::Event);
        }

        // Scan by State type - should get 1 key
        let state_results = store.scan_by_type(TypeTag::State, u64::MAX).unwrap();
        assert_eq!(state_results.len(), 1);
        for (key, _) in &state_results {
            assert_eq!(key.type_tag, TypeTag::State);
        }
    }

    #[test]
    fn test_scan_by_type_respects_max_version() {
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
            .put(Key::new_kv(ns.clone(), "kv1"), Value::Int(1), None)
            .unwrap();

        // Insert at version 2
        store
            .put(Key::new_kv(ns.clone(), "kv2"), Value::Int(2), None)
            .unwrap();

        // Insert at version 3
        store
            .put(Key::new_kv(ns.clone(), "kv3"), Value::Int(3), None)
            .unwrap();

        // Scan with max_version=1 - should only return kv1
        let results = store.scan_by_type(TypeTag::KV, 1).unwrap();
        assert_eq!(results.len(), 1);

        // Scan with max_version=2 - should return kv1 and kv2
        let results = store.scan_by_type(TypeTag::KV, 2).unwrap();
        assert_eq!(results.len(), 2);

        // Scan with max_version=MAX - should return all 3
        let results = store.scan_by_type(TypeTag::KV, u64::MAX).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_indices_stay_consistent() {
        // Test that indices are updated correctly on put and delete
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key1 = Key::new_kv(ns.clone(), "key1");
        let key2 = Key::new_event(ns.clone(), 1);

        // Initially, indices should be empty
        assert_eq!(store.scan_by_run(run_id, u64::MAX).unwrap().len(), 0);
        assert_eq!(store.scan_by_type(TypeTag::KV, u64::MAX).unwrap().len(), 0);
        assert_eq!(
            store.scan_by_type(TypeTag::Event, u64::MAX).unwrap().len(),
            0
        );

        // Insert keys
        store.put(key1.clone(), Value::Int(1), None).unwrap();
        store.put(key2.clone(), Value::Int(2), None).unwrap();

        // Indices should reflect the inserts
        assert_eq!(store.scan_by_run(run_id, u64::MAX).unwrap().len(), 2);
        assert_eq!(store.scan_by_type(TypeTag::KV, u64::MAX).unwrap().len(), 1);
        assert_eq!(
            store.scan_by_type(TypeTag::Event, u64::MAX).unwrap().len(),
            1
        );

        // Delete one key
        store.delete(&key1).unwrap();

        // Indices should reflect the delete
        assert_eq!(store.scan_by_run(run_id, u64::MAX).unwrap().len(), 1);
        assert_eq!(store.scan_by_type(TypeTag::KV, u64::MAX).unwrap().len(), 0);
        assert_eq!(
            store.scan_by_type(TypeTag::Event, u64::MAX).unwrap().len(),
            1
        );

        // Delete the other key
        store.delete(&key2).unwrap();

        // Both indices should be empty
        assert_eq!(store.scan_by_run(run_id, u64::MAX).unwrap().len(), 0);
        assert_eq!(
            store.scan_by_type(TypeTag::Event, u64::MAX).unwrap().len(),
            0
        );
    }

    #[test]
    fn test_overwrite_does_not_duplicate_index_entries() {
        // Test that overwriting a key doesn't create duplicate index entries
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key = Key::new_kv(ns.clone(), "key1");

        // Insert the key multiple times (overwrite)
        store.put(key.clone(), Value::Int(1), None).unwrap();
        store.put(key.clone(), Value::Int(2), None).unwrap();
        store.put(key.clone(), Value::Int(3), None).unwrap();

        // run_index should have only 1 entry for this key
        let run_results = store.scan_by_run(run_id, u64::MAX).unwrap();
        assert_eq!(run_results.len(), 1);
        assert_eq!(run_results[0].1.value, Value::Int(3)); // Latest value

        // type_index should have only 1 entry for this key
        let type_results = store.scan_by_type(TypeTag::KV, u64::MAX).unwrap();
        assert_eq!(type_results.len(), 1);
    }

    // ========================================
    // TTL Index Tests (Story #14)
    // ========================================

    #[test]
    fn test_ttl_index_insert_and_find_expired() {
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Put key with TTL
        let key = Key::new_kv(ns.clone(), "temp");
        store
            .put(
                key.clone(),
                Value::Bytes(b"data".to_vec()),
                Some(Duration::from_secs(60)),
            )
            .unwrap();

        // Key should exist
        assert!(store.get(&key).unwrap().is_some());

        // Key should not appear in expired list (not expired yet)
        let expired = store.find_expired_keys().unwrap();
        assert!(!expired.contains(&key));
    }

    #[test]
    fn test_ttl_index_removed_on_delete() {
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Put key with TTL
        let key = Key::new_kv(ns.clone(), "temp");
        store
            .put(
                key.clone(),
                Value::Bytes(b"data".to_vec()),
                Some(Duration::from_secs(60)),
            )
            .unwrap();

        // Delete the key
        store.delete(&key).unwrap();

        // TTL index should be empty now
        let ttl_idx = store.ttl_index.read();
        assert!(ttl_idx.is_empty());
    }

    #[test]
    fn test_ttl_index_updated_on_overwrite() {
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key = Key::new_kv(ns.clone(), "temp");

        // Put with TTL
        store
            .put(key.clone(), Value::Int(1), Some(Duration::from_secs(60)))
            .unwrap();

        // Overwrite with different TTL
        store
            .put(key.clone(), Value::Int(2), Some(Duration::from_secs(120)))
            .unwrap();

        // TTL index should have only 1 entry (old entry should be removed)
        let ttl_idx = store.ttl_index.read();
        assert_eq!(ttl_idx.len(), 1);
    }

    #[test]
    fn test_ttl_index_not_updated_for_non_ttl_keys() {
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Put key without TTL
        let key = Key::new_kv(ns.clone(), "persistent");
        store.put(key.clone(), Value::Int(1), None).unwrap();

        // TTL index should be empty
        let ttl_idx = store.ttl_index.read();
        assert!(ttl_idx.is_empty());
    }

    #[test]
    fn test_ttl_overwrite_from_ttl_to_no_ttl() {
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key = Key::new_kv(ns.clone(), "temp");

        // Put with TTL
        store
            .put(key.clone(), Value::Int(1), Some(Duration::from_secs(60)))
            .unwrap();

        // TTL index should have entry
        {
            let ttl_idx = store.ttl_index.read();
            assert_eq!(ttl_idx.len(), 1);
        }

        // Overwrite without TTL
        store.put(key.clone(), Value::Int(2), None).unwrap();

        // TTL index should be empty (old entry removed, no new entry added)
        let ttl_idx = store.ttl_index.read();
        assert!(ttl_idx.is_empty());
    }

    #[test]
    fn test_find_expired_keys_uses_index() {
        // This test verifies that find_expired_keys uses the ttl_index efficiently.
        // We add multiple keys with different TTLs and verify only expired ones are returned.
        let store = UnifiedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Add key with short TTL (1 second)
        let short_key = Key::new_kv(ns.clone(), "short");
        store
            .put(
                short_key.clone(),
                Value::Int(1),
                Some(Duration::from_secs(1)),
            )
            .unwrap();

        // Add key with long TTL (60 seconds)
        let long_key = Key::new_kv(ns.clone(), "long");
        store
            .put(
                long_key.clone(),
                Value::Int(2),
                Some(Duration::from_secs(60)),
            )
            .unwrap();

        // Add key with no TTL
        let no_ttl_key = Key::new_kv(ns.clone(), "no_ttl");
        store.put(no_ttl_key.clone(), Value::Int(3), None).unwrap();

        // Wait for short TTL to expire
        thread::sleep(Duration::from_millis(1100));

        // Find expired keys - should only include short_key
        let expired = store.find_expired_keys().unwrap();
        assert_eq!(expired.len(), 1);
        assert!(expired.contains(&short_key));
        assert!(!expired.contains(&long_key));
        assert!(!expired.contains(&no_ttl_key));
    }
}
