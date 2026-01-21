//! ClonedSnapshotView: MVP snapshot implementation via deep clone
//!
//! This module provides version-bounded views of storage for transaction isolation.
//! The MVP implementation creates a deep clone of the BTreeMap at snapshot time.
//!
//! # Design Notes
//!
//! - **Deep clone**: Expensive but correct for MVP (full BTreeMap copy)
//! - **Immutable**: Once created, the snapshot never changes
//! - **Thread-safe**: Can be safely shared across threads (Arc-wrapped data)
//! - **Version-bounded**: Only returns data visible at snapshot version
//!
//! # Future Optimization
//!
//! The `SnapshotView` trait abstraction allows replacing this with a lazy
//! implementation (LazySnapshotView) that reads from live storage with
//! version filtering, avoiding the clone cost.

use std::collections::BTreeMap;
use std::sync::Arc;

use strata_core::{Key, Result, SnapshotView, VersionedValue};

use crate::stored_value::StoredValue;

/// A snapshot view that clones the entire BTreeMap
///
/// This is the MVP implementation - simple but expensive.
/// Creates an immutable point-in-time view of storage.
///
/// # Example
///
/// ```ignore
/// let store = UnifiedStore::new();
/// // ... write some data ...
/// let snapshot = store.create_snapshot();
///
/// // Writes after snapshot creation are not visible
/// store.put(key, value, None);
/// assert!(snapshot.get(&key).unwrap().is_none());
/// ```
#[derive(Debug, Clone)]
pub struct ClonedSnapshotView {
    /// The version at which this snapshot was created
    version: u64,
    /// Deep clone of the storage data at snapshot time (includes TTL info)
    data: Arc<BTreeMap<Key, StoredValue>>,
}

impl ClonedSnapshotView {
    /// Create a new ClonedSnapshotView from existing data
    ///
    /// # Arguments
    ///
    /// * `version` - The version at which the snapshot was created
    /// * `data` - The cloned BTreeMap data
    ///
    /// # Note
    ///
    /// This is typically called by `UnifiedStore::create_snapshot()`, not directly.
    pub fn new(version: u64, data: BTreeMap<Key, StoredValue>) -> Self {
        Self {
            version,
            data: Arc::new(data),
        }
    }
}

impl SnapshotView for ClonedSnapshotView {
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>> {
        match self.data.get(key) {
            Some(sv) if sv.version().as_u64() <= self.version && !sv.is_expired() => {
                Ok(Some(sv.versioned().clone()))
            }
            _ => Ok(None),
        }
    }

    fn scan_prefix(&self, prefix: &Key) -> Result<Vec<(Key, VersionedValue)>> {
        let results: Vec<(Key, VersionedValue)> = self
            .data
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .filter(|(_, sv)| sv.version().as_u64() <= self.version && !sv.is_expired())
            .map(|(k, sv)| (k.clone(), sv.versioned().clone()))
            .collect();

        Ok(results)
    }

    fn version(&self) -> u64 {
        self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UnifiedStore;
    use strata_core::{Namespace, RunId, Storage, TypeTag, Value};

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
    // Test 1: Snapshot Creation
    // ========================================

    #[test]
    fn test_snapshot_creation() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        // Write some data
        store
            .put(test_key(&ns, "key1"), Value::I64(1), None)
            .unwrap();
        store
            .put(test_key(&ns, "key2"), Value::I64(2), None)
            .unwrap();
        store
            .put(test_key(&ns, "key3"), Value::I64(3), None)
            .unwrap();

        // Create snapshot
        let snapshot = store.create_snapshot();

        // Snapshot version should match store's current version at creation time
        assert_eq!(snapshot.version(), 3);
        assert_eq!(store.current_version(), 3);
    }

    // ========================================
    // Test 2: Snapshot Get
    // ========================================

    #[test]
    fn test_snapshot_get() {
        use strata_core::Version;

        let store = UnifiedStore::new();
        let ns = test_namespace();

        let key = test_key(&ns, "frozen_key");
        let value = Value::String("frozen_value".to_string());

        // Write data
        store.put(key.clone(), value.clone(), None).unwrap();

        // Create snapshot
        let snapshot = store.create_snapshot();

        // Snapshot should be able to read the data
        let result = snapshot.get(&key).unwrap();
        assert!(result.is_some());

        let vv = result.unwrap();
        assert_eq!(vv.value, value);
        assert_eq!(vv.version, Version::txn(1));
    }

    // ========================================
    // Test 3: Snapshot Isolation
    // ========================================

    #[test]
    fn test_snapshot_isolation() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        let key1 = test_key(&ns, "before_snapshot");
        let key2 = test_key(&ns, "after_snapshot");

        // Write data before snapshot
        store.put(key1.clone(), Value::I64(1), None).unwrap();

        // Create snapshot at version 1
        let snapshot = store.create_snapshot();
        assert_eq!(snapshot.version(), 1);

        // Write data after snapshot
        store.put(key2.clone(), Value::I64(2), None).unwrap();

        // Also update existing key with new value
        store.put(key1.clone(), Value::I64(100), None).unwrap();

        // Verify store sees new data
        assert_eq!(store.current_version(), 3);
        let store_result = store.get(&key2).unwrap();
        assert!(store_result.is_some());

        // Snapshot should NOT see data written after creation
        let snap_result = snapshot.get(&key2).unwrap();
        assert!(
            snap_result.is_none(),
            "Snapshot should not see keys added after creation"
        );

        // Snapshot should still see OLD value for key1 (not the updated value)
        // Note: In MVP, we clone the data at snapshot time, so we see version 1's data
        let snap_key1 = snapshot.get(&key1).unwrap();
        assert!(snap_key1.is_some());
        assert_eq!(snap_key1.unwrap().value, Value::I64(1)); // Original value, not 100
    }

    // ========================================
    // Test 4: Snapshot Scan Prefix
    // ========================================

    #[test]
    fn test_snapshot_scan_prefix() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        // Write data with "user:" prefix
        store
            .put(test_key(&ns, "user:alice"), Value::I64(1), None)
            .unwrap();
        store
            .put(test_key(&ns, "user:bob"), Value::I64(2), None)
            .unwrap();
        store
            .put(test_key(&ns, "config:db"), Value::I64(100), None)
            .unwrap();

        // Create snapshot
        let snapshot = store.create_snapshot();

        // Add more user keys after snapshot
        store
            .put(test_key(&ns, "user:charlie"), Value::I64(3), None)
            .unwrap();

        // Scan prefix in snapshot
        let prefix = test_key(&ns, "user:");
        let results = snapshot.scan_prefix(&prefix).unwrap();

        // Should only see alice and bob (not charlie, added after snapshot)
        assert_eq!(results.len(), 2);

        let keys: Vec<String> = results
            .iter()
            .map(|(k, _)| String::from_utf8_lossy(&k.user_key).to_string())
            .collect();
        assert!(keys.contains(&"user:alice".to_string()));
        assert!(keys.contains(&"user:bob".to_string()));
        assert!(!keys.contains(&"user:charlie".to_string()));
    }

    // ========================================
    // Test 5: Snapshot Is Immutable
    // ========================================

    #[test]
    fn test_snapshot_is_immutable() {
        use std::sync::Arc;
        use std::thread;

        let store = Arc::new(UnifiedStore::new());
        let ns = test_namespace();

        // Write initial data
        store
            .put(test_key(&ns, "stable_key"), Value::I64(42), None)
            .unwrap();

        // Create snapshot
        let snapshot = Arc::new(store.create_snapshot());

        // Spawn multiple readers to verify they all see the same data
        let mut handles = vec![];
        for _ in 0..10 {
            let snapshot = Arc::clone(&snapshot);
            let ns = ns.clone();

            let handle = thread::spawn(move || {
                let key = test_key(&ns, "stable_key");
                let result = snapshot.get(&key).unwrap();
                assert!(result.is_some());
                assert_eq!(result.unwrap().value, Value::I64(42));
                snapshot.version()
            });

            handles.push(handle);
        }

        // Concurrently write to the store (should not affect snapshot)
        let store_writer = Arc::clone(&store);
        let ns_writer = ns.clone();
        let writer_handle = thread::spawn(move || {
            for i in 0..100 {
                store_writer
                    .put(
                        test_key(&ns_writer, &format!("new_key_{}", i)),
                        Value::I64(i),
                        None,
                    )
                    .unwrap();
            }
        });

        // Collect results from readers
        let versions: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All readers should see the same snapshot version
        assert!(versions.iter().all(|&v| v == 1));

        // Wait for writer
        writer_handle.join().unwrap();

        // Original snapshot still sees version 1
        assert_eq!(snapshot.version(), 1);

        // But store has progressed
        assert_eq!(store.current_version(), 101); // 1 initial + 100 new
    }

    // ========================================
    // Additional Tests
    // ========================================

    #[test]
    fn test_snapshot_empty_store() {
        let store = UnifiedStore::new();

        // Snapshot of empty store
        let snapshot = store.create_snapshot();

        assert_eq!(snapshot.version(), 0);

        // Get on empty snapshot returns None
        let ns = test_namespace();
        let key = test_key(&ns, "nonexistent");
        let result = snapshot.get(&key).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_snapshot_can_be_cloned() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        store
            .put(test_key(&ns, "key1"), Value::I64(1), None)
            .unwrap();

        let snapshot1 = store.create_snapshot();
        let snapshot2 = snapshot1.clone();

        // Both snapshots should have same version
        assert_eq!(snapshot1.version(), snapshot2.version());

        // Both should return same data
        let key = test_key(&ns, "key1");
        assert_eq!(snapshot1.get(&key).unwrap(), snapshot2.get(&key).unwrap());
    }

    #[test]
    fn test_snapshot_respects_type_tags() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        // Insert different types
        let kv_key = Key::new_kv(ns.clone(), "data");
        let event_key = Key::new_event(ns.clone(), 1);

        store.put(kv_key.clone(), Value::I64(1), None).unwrap();
        store.put(event_key.clone(), Value::I64(2), None).unwrap();

        let snapshot = store.create_snapshot();

        // Can get both types
        assert!(snapshot.get(&kv_key).unwrap().is_some());
        assert!(snapshot.get(&event_key).unwrap().is_some());

        // Scan prefix for KV should not return Event
        let prefix = Key::new_kv(ns.clone(), "");
        let results = snapshot.scan_prefix(&prefix).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0.type_tag, TypeTag::KV);
    }

    #[test]
    fn test_multiple_snapshots_at_different_versions() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        let key = test_key(&ns, "evolving_key");

        // Version 1
        store.put(key.clone(), Value::I64(1), None).unwrap();
        let snapshot_v1 = store.create_snapshot();

        // Version 2
        store.put(key.clone(), Value::I64(2), None).unwrap();
        let snapshot_v2 = store.create_snapshot();

        // Version 3
        store.put(key.clone(), Value::I64(3), None).unwrap();
        let snapshot_v3 = store.create_snapshot();

        // Each snapshot has different version
        assert_eq!(snapshot_v1.version(), 1);
        assert_eq!(snapshot_v2.version(), 2);
        assert_eq!(snapshot_v3.version(), 3);

        // Each snapshot sees its own version's value
        // Note: MVP clones data, so each snapshot has the value at clone time
        assert_eq!(snapshot_v1.get(&key).unwrap().unwrap().value, Value::I64(1));
        assert_eq!(snapshot_v2.get(&key).unwrap().unwrap().value, Value::I64(2));
        assert_eq!(snapshot_v3.get(&key).unwrap().unwrap().value, Value::I64(3));
    }

    #[test]
    fn test_snapshot_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<ClonedSnapshotView>();
        assert_sync::<ClonedSnapshotView>();
    }

    #[test]
    fn test_snapshot_trait_object() {
        let store = UnifiedStore::new();
        let ns = test_namespace();

        store
            .put(test_key(&ns, "key1"), Value::I64(1), None)
            .unwrap();

        let snapshot = store.create_snapshot();

        // Can use as trait object
        fn use_snapshot(snap: &dyn SnapshotView) -> u64 {
            snap.version()
        }

        assert_eq!(use_snapshot(&snapshot), 1);
    }
}
