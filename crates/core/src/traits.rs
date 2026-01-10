//! Core traits for storage and snapshot abstraction
//!
//! This module defines the Storage and SnapshotView traits that enable
//! swapping implementations without breaking upper layers.

// Note: We use () as placeholder for error type until Story #10 implements proper error types
#![allow(clippy::result_unit_err)]

use std::time::Duration;

/// Storage abstraction for unified backend
///
/// This trait enables replacing the MVP BTreeMap+RwLock implementation
/// with sharded, lock-free, or distributed storage without breaking
/// upper layers (concurrency, primitives, engine).
///
/// Thread safety: All methods must be safe to call concurrently from
/// multiple threads (requires Send + Sync).
///
/// # Examples
///
/// ```
/// use in_mem_core::traits::Storage;
/// // Storage implementations will be added in Epic 2
/// ```
pub trait Storage: Send + Sync {
    /// Get current value for key (latest version)
    ///
    /// Returns None if key doesn't exist or is expired.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>>;

    /// Get value at or before specified version (for snapshot isolation)
    ///
    /// This enables creating snapshots without cloning the entire store.
    /// Returns the latest version <= max_version.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    fn get_versioned(&self, key: &Key, max_version: u64) -> Result<Option<VersionedValue>>;

    /// Put key-value pair with optional TTL
    ///
    /// Returns the version assigned to this write.
    /// Version is monotonically increasing and assigned by storage layer.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> Result<u64>;

    /// Delete key
    ///
    /// Returns the deleted value if it existed.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    fn delete(&self, key: &Key) -> Result<Option<VersionedValue>>;

    /// Scan keys with given prefix at or before max_version
    ///
    /// Results are sorted by key order (namespace → type_tag → user_key).
    /// Used for range queries and namespace scans.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    fn scan_prefix(&self, prefix: &Key, max_version: u64) -> Result<Vec<(Key, VersionedValue)>>;

    /// Scan all keys for a given run_id at or before max_version
    ///
    /// Critical for replay: fetch all writes for a specific run.
    /// Results are sorted by key order.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    fn scan_by_run(&self, run_id: RunId, max_version: u64) -> Result<Vec<(Key, VersionedValue)>>;

    /// Get current global version
    ///
    /// Returns the highest version assigned so far.
    /// Used for creating snapshots at current version.
    fn current_version(&self) -> u64;
}

/// Snapshot view abstraction for snapshot isolation
///
/// Provides version-bounded read view of storage.
/// MVP: ClonedSnapshotView (deep clone at version)
/// Future: LazySnapshotView (version-bounded reads from live storage)
///
/// Thread safety: Must be safe to pass between threads (Send + Sync).
///
/// # Examples
///
/// ```
/// use in_mem_core::traits::SnapshotView;
/// // SnapshotView implementations will be added in Epic 2
/// ```
pub trait SnapshotView: Send + Sync {
    /// Get value from snapshot
    ///
    /// Returns value as it existed at snapshot version.
    /// Returns None if key didn't exist at that version.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>>;

    /// Scan keys with prefix from snapshot
    ///
    /// Returns all matching keys as they existed at snapshot version.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage operation fails.
    fn scan_prefix(&self, prefix: &Key) -> Result<Vec<(Key, VersionedValue)>>;

    /// Get snapshot version
    ///
    /// Returns the version this snapshot was created at.
    fn version(&self) -> u64;
}

// Type aliases for convenience (these types will be defined in other stories)
// For now, we use placeholder types that will be replaced when the actual types are implemented

/// Placeholder for Key type (will be defined in Story #8)
pub type Key = ();

/// Placeholder for Value type (will be defined in Story #9)
pub type Value = ();

/// Placeholder for VersionedValue type (will be defined in Story #9)
pub type VersionedValue = ();

/// Placeholder for RunId type (will be defined in Story #7)
pub type RunId = ();

/// Placeholder for Result type (will be defined in Story #10)
pub type Result<T> = std::result::Result<T, ()>;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that Storage can be used as a trait object
    ///
    /// This ensures the trait is object-safe, which means it can be used
    /// with dynamic dispatch (dyn Storage).
    #[test]
    fn test_storage_trait_object() {
        // This function accepts any type that implements Storage as a trait object
        fn accepts_storage(_storage: &dyn Storage) {
            // If this compiles, Storage is object-safe
        }

        // The function exists and compiles, proving trait object works
        let _ = accepts_storage as fn(&dyn Storage);
    }

    /// Test that SnapshotView can be used as a trait object
    ///
    /// This ensures the trait is object-safe for dynamic dispatch.
    #[test]
    fn test_snapshot_trait_object() {
        // This function accepts any type that implements SnapshotView as a trait object
        fn accepts_snapshot(_snapshot: &dyn SnapshotView) {
            // If this compiles, SnapshotView is object-safe
        }

        // The function exists and compiles, proving trait object works
        let _ = accepts_snapshot as fn(&dyn SnapshotView);
    }

    /// Test that Storage requires Send + Sync
    ///
    /// This ensures Storage implementations can be safely shared across threads.
    #[test]
    fn test_storage_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        // These will only compile if dyn Storage is Send + Sync
        assert_send::<Box<dyn Storage>>();
        assert_sync::<Box<dyn Storage>>();
    }

    /// Test that SnapshotView requires Send + Sync
    ///
    /// This ensures SnapshotView implementations can be safely shared across threads.
    #[test]
    fn test_snapshot_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        // These will only compile if dyn SnapshotView is Send + Sync
        assert_send::<Box<dyn SnapshotView>>();
        assert_sync::<Box<dyn SnapshotView>>();
    }
}
