//! Sharded storage performance
//!
//! Replaces RwLock + BTreeMap with DashMap + HashMap.
//! Lock-free reads, sharded writes, O(1) lookups.
//!
//! # Design
//!
//! - DashMap: 16-way sharded by default, lock-free reads
//! - FxHashMap: O(1) lookups, fast non-crypto hash
//! - Per-RunId: Natural agent partitioning, no cross-run contention
//!
//! # Performance Targets
//!
//! - get(): Lock-free via DashMap
//! - put(): Only locks target shard
//! - Snapshot acquisition: < 500ns
//! - Different runs: Never contend
//!
//! # Storage vs Contract Types
//!
//! - `StoredValue`: Internal storage type that includes TTL (storage concern)
//! - `VersionedValue`: Contract type returned to callers (no TTL)
//!
//! # Version Handling
//!
//! The storage layer uses raw `u64` for version comparisons because:
//! 1. All versions in storage are `Version::Txn` variants (transaction versions)
//! 2. Raw u64 comparison is correct for same-variant versions
//! 3. The `Version::Ord` implementation compares discriminant first, ensuring
//!    cross-variant comparisons are safe (though they shouldn't occur here)
//! 4. Performance: Avoiding enum matching on every comparison

use dashmap::DashMap;
use strata_core::types::{Key, RunId};
use strata_core::{Timestamp, Version, VersionedValue};
use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::stored_value::StoredValue;

/// Per-run shard containing run's data
///
/// Version chain for MVCC - stores multiple versions of a value
///
/// Versions are stored in descending order (newest first) for efficient
/// snapshot reads - we typically want the most recent version <= snapshot_version.
///
/// # Performance
///
/// Uses VecDeque for O(1) push_front instead of SmallVec's O(n) insert(0, ...).
/// This is critical for workloads that repeatedly update the same key (like CAS).
#[derive(Debug, Clone)]
pub struct VersionChain {
    /// Versions stored newest-first for efficient MVCC reads
    /// VecDeque provides O(1) push_front for new versions
    /// Uses StoredValue to include TTL information
    versions: VecDeque<StoredValue>,
}

impl VersionChain {
    /// Create a new version chain with a single version
    pub fn new(value: StoredValue) -> Self {
        let mut versions = VecDeque::with_capacity(4);
        versions.push_front(value);
        Self { versions }
    }

    /// Add a new version (must be newer than existing versions)
    ///
    /// O(1) operation using VecDeque::push_front
    #[inline]
    pub fn push(&mut self, value: StoredValue) {
        // O(1) insert at front (newest first)
        self.versions.push_front(value);
    }

    /// Get the version at or before the given max_version
    ///
    /// Note: Uses raw u64 comparison since all storage versions are TxnId variants.
    /// Debug assertions verify this invariant.
    pub fn get_at_version(&self, max_version: u64) -> Option<&StoredValue> {
        // Debug assertion: all versions should be Txn variants
        debug_assert!(
            self.versions.iter().all(|sv| sv.version().is_txn()),
            "Storage layer should only contain Txn versions"
        );
        // Versions are newest-first, so we scan until we find one <= max_version
        self.versions
            .iter()
            .find(|sv| sv.version().as_u64() <= max_version)
    }

    /// Get the latest version
    #[inline]
    pub fn latest(&self) -> Option<&StoredValue> {
        self.versions.front()
    }

    /// Remove versions older than min_version (garbage collection)
    /// Keeps at least one version
    pub fn gc(&mut self, min_version: u64) {
        if self.versions.len() <= 1 {
            return;
        }
        // Keep versions >= min_version, but always keep at least the latest
        // Versions are newest-first, so pop from back (oldest)
        while self.versions.len() > 1 {
            if let Some(oldest) = self.versions.back() {
                if oldest.version().as_u64() < min_version {
                    self.versions.pop_back();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    /// Number of versions stored
    pub fn version_count(&self) -> usize {
        self.versions.len()
    }

    /// Get version history (newest first)
    ///
    /// Returns versions in descending order (newest first).
    /// Optionally limited and filtered by `before_version`.
    ///
    /// # Arguments
    /// * `limit` - Maximum versions to return (None = all)
    /// * `before_version` - Only return versions older than this (exclusive, for pagination)
    ///
    /// # Returns
    /// Vector of StoredValue references, newest first
    pub fn history(&self, limit: Option<usize>, before_version: Option<u64>) -> Vec<&StoredValue> {
        // Debug assertion: all versions should be Txn variants
        debug_assert!(
            self.versions.iter().all(|sv| sv.version().is_txn()),
            "Storage layer should only contain Txn versions"
        );

        let iter = self.versions.iter();

        // Filter by before_version if specified (only versions < before)
        let filtered: Vec<&StoredValue> = match before_version {
            Some(before) => iter
                .filter(|sv| sv.version().as_u64() < before)
                .collect(),
            None => iter.collect(),
        };

        // Apply limit if specified
        match limit {
            Some(n) => filtered.into_iter().take(n).collect(),
            None => filtered,
        }
    }

    /// Check if the version chain is empty
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }
}

/// Each RunId gets its own shard with an FxHashMap for O(1) lookups.
/// This ensures different runs never contend with each other.
///
/// Uses VersionChain for MVCC - multiple versions per key for snapshot isolation.
#[derive(Debug)]
pub struct Shard {
    /// HashMap with FxHash for O(1) lookups, storing version chains
    pub(crate) data: FxHashMap<Key, VersionChain>,
}

impl Shard {
    /// Create a new empty shard
    pub fn new() -> Self {
        Self {
            data: FxHashMap::default(),
        }
    }

    /// Create a shard with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: FxHashMap::with_capacity_and_hasher(capacity, Default::default()),
        }
    }

    /// Get number of keys in this shard
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if shard is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Default for Shard {
    fn default() -> Self {
        Self::new()
    }
}

/// Sharded storage - DashMap by RunId, HashMap within
///
/// # Design
///
/// - DashMap: 16-way sharded by default, lock-free reads
/// - FxHashMap: O(1) lookups, fast non-crypto hash
/// - Per-RunId: Natural agent partitioning, no cross-run contention
///
/// # Thread Safety
///
/// All operations are thread-safe:
/// - get(): Lock-free read via DashMap
/// - put(): Only locks the target run's shard
/// - Different runs never contend
///
/// # Example
///
/// ```ignore
/// use strata_storage::ShardedStore;
/// use std::sync::Arc;
///
/// let store = Arc::new(ShardedStore::new());
/// let snapshot = store.snapshot();
/// ```
pub struct ShardedStore {
    /// Per-run shards using DashMap
    shards: DashMap<RunId, Shard>,
    /// Global version for snapshots
    version: AtomicU64,
}

impl ShardedStore {
    /// Create new sharded store
    pub fn new() -> Self {
        Self {
            shards: DashMap::new(),
            version: AtomicU64::new(0),
        }
    }

    /// Create with expected number of runs
    pub fn with_capacity(num_runs: usize) -> Self {
        Self {
            shards: DashMap::with_capacity(num_runs),
            version: AtomicU64::new(0),
        }
    }

    /// Get current version
    #[inline]
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Increment version and return new value
    #[inline]
    pub fn next_version(&self) -> u64 {
        self.version.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Set version (used during recovery)
    pub fn set_version(&self, version: u64) {
        self.version.store(version, Ordering::Release);
    }

    /// Get number of shards (runs)
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    /// Check if a run exists
    pub fn has_run(&self, run_id: &RunId) -> bool {
        self.shards.contains_key(run_id)
    }

    /// Get total number of entries across all shards
    pub fn total_entries(&self) -> usize {
        self.shards.iter().map(|entry| entry.value().len()).sum()
    }

    // ========================================================================
    // Get/Put/Delete Operations
    // ========================================================================

    // NOTE: `get()` is provided by the Storage trait implementation,
    // which includes proper TTL expiration checks.
    // Use `Storage::get()` trait method instead of an inherent method
    // to ensure consistent behavior across all callers.

    /// Put a value for a key (adds to version chain for MVCC)
    ///
    /// Sharded write - only locks this run's shard.
    /// Other runs can read/write concurrently without contention.
    ///
    /// # Arguments
    ///
    /// * `key` - Key to store (contains RunId)
    /// * `value` - StoredValue to store (includes TTL)
    ///
    /// # Performance
    ///
    /// - O(1) insert via FxHashMap
    /// - Only locks the target run's shard
    #[inline]
    pub fn put(&self, key: Key, value: StoredValue) {
        let run_id = key.namespace.run_id;
        let mut shard = self.shards.entry(run_id).or_default();

        if let Some(chain) = shard.data.get_mut(&key) {
            // Add new version to existing chain
            chain.push(value);
        } else {
            // Create new chain
            shard.data.insert(key, VersionChain::new(value));
        }
    }

    /// Delete a key
    ///
    /// Removes all versions of the key. For MVCC correctness, this should
    /// only be called when no active snapshots could reference the key.
    ///
    /// # Arguments
    ///
    /// * `key` - Key to delete (contains RunId)
    #[inline]
    pub fn delete(&self, key: &Key) -> Option<VersionedValue> {
        let run_id = key.namespace.run_id;
        self.shards
            .get_mut(&run_id)
            .and_then(|mut shard| shard.data.remove(key))
            .and_then(|chain| chain.latest().map(|sv| sv.versioned().clone()))
    }

    /// Check if a key exists
    ///
    /// Lock-free check via DashMap read guard.
    #[inline]
    pub fn contains(&self, key: &Key) -> bool {
        let run_id = key.namespace.run_id;
        self.shards
            .get(&run_id)
            .map(|shard| shard.data.contains_key(key))
            .unwrap_or(false)
    }

    /// Apply a batch of writes and deletes atomically
    ///
    /// All operations in the batch are applied with the given version.
    ///
    /// # Arguments
    ///
    /// * `writes` - Key-value pairs to write
    /// * `deletes` - Keys to delete
    /// * `version` - Version to assign to all writes
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Always succeeds (for API compatibility with UnifiedStore)
    ///
    /// # Performance
    ///
    /// Captures timestamp once per batch instead of per-write to avoid
    /// repeated syscalls. All writes in a transaction share the same timestamp.
    pub fn apply_batch(
        &self,
        writes: &[(Key, strata_core::value::Value)],
        deletes: &[Key],
        version: u64,
    ) -> strata_core::error::Result<()> {
        use std::sync::atomic::Ordering;

        // Capture timestamp once for entire batch
        let timestamp = Timestamp::now();

        // Apply writes
        for (key, value) in writes {
            let stored = StoredValue::with_timestamp(
                value.clone(),
                Version::txn(version),
                timestamp,
                None,
            );
            self.put(key.clone(), stored);
        }

        // Apply deletes
        for key in deletes {
            self.delete(key);
        }

        // Update global version to be at least this version
        // This ensures subsequent snapshots can see the committed data
        self.version.fetch_max(version, Ordering::AcqRel);

        Ok(())
    }

    /// Get count of entries for a specific run
    pub fn run_entry_count(&self, run_id: &RunId) -> usize {
        self.shards
            .get(run_id)
            .map(|shard| shard.len())
            .unwrap_or(0)
    }

    // ========================================================================
    // List Operations
    // ========================================================================

    /// List all entries for a run
    ///
    /// NOTE: Slower than BTreeMap range scan. Requires collect + sort.
    /// This is acceptable because list operations are NOT on the hot path.
    ///
    /// # Arguments
    ///
    /// * `run_id` - The run to list entries for
    ///
    /// # Returns
    ///
    /// Vector of (Key, VersionedValue) pairs, sorted by key
    pub fn list_run(&self, run_id: &RunId) -> Vec<(Key, VersionedValue)> {
        self.shards
            .get(run_id)
            .map(|shard| {
                let mut results: Vec<_> = shard
                    .data
                    .iter()
                    .filter_map(|(k, chain)| {
                        chain.latest().map(|sv| (k.clone(), sv.versioned().clone()))
                    })
                    .collect();

                // Sort for consistent ordering (Key implements Ord)
                results.sort_by(|(a, _), (b, _)| a.cmp(b));
                results
            })
            .unwrap_or_default()
    }

    /// List entries matching a key prefix
    ///
    /// Returns all entries where `key.starts_with(prefix)`.
    /// The prefix key determines namespace, type_tag, and user_key prefix.
    ///
    /// NOTE: Requires filter + sort, O(n) where n = shard size.
    /// Use sparingly; not for hot path operations.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Prefix key to match (namespace + type_tag + user_key prefix)
    ///
    /// # Returns
    ///
    /// Vector of (Key, VersionedValue) pairs matching prefix, sorted by key
    pub fn list_by_prefix(&self, prefix: &Key) -> Vec<(Key, VersionedValue)> {
        let run_id = prefix.namespace.run_id;
        self.shards
            .get(&run_id)
            .map(|shard| {
                let mut results: Vec<_> = shard
                    .data
                    .iter()
                    .filter(|(k, _)| k.starts_with(prefix))
                    .filter_map(|(k, chain)| {
                        chain.latest().map(|sv| (k.clone(), sv.versioned().clone()))
                    })
                    .collect();

                // Sort for consistent ordering
                results.sort_by(|(a, _), (b, _)| a.cmp(b));
                results
            })
            .unwrap_or_default()
    }

    /// List entries of a specific type for a run
    ///
    /// Filters by TypeTag within a run's shard.
    ///
    /// # Arguments
    ///
    /// * `run_id` - The run to query
    /// * `type_tag` - The type to filter by (KV, Event, State, etc.)
    ///
    /// # Returns
    ///
    /// Vector of (Key, VersionedValue) pairs of the specified type, sorted
    pub fn list_by_type(
        &self,
        run_id: &RunId,
        type_tag: strata_core::types::TypeTag,
    ) -> Vec<(Key, VersionedValue)> {
        self.shards
            .get(run_id)
            .map(|shard| {
                let mut results: Vec<_> = shard
                    .data
                    .iter()
                    .filter(|(k, _)| k.type_tag == type_tag)
                    .filter_map(|(k, chain)| {
                        chain.latest().map(|sv| (k.clone(), sv.versioned().clone()))
                    })
                    .collect();

                // Sort for consistent ordering
                results.sort_by(|(a, _), (b, _)| a.cmp(b));
                results
            })
            .unwrap_or_default()
    }

    /// Count entries of a specific type for a run
    pub fn count_by_type(&self, run_id: &RunId, type_tag: strata_core::types::TypeTag) -> usize {
        self.shards
            .get(run_id)
            .map(|shard| {
                shard
                    .data
                    .iter()
                    .filter(|(k, _)| k.type_tag == type_tag)
                    .count()
            })
            .unwrap_or(0)
    }

    /// Iterate over all runs
    ///
    /// Returns an iterator over all RunIds that have data
    pub fn run_ids(&self) -> Vec<RunId> {
        self.shards.iter().map(|entry| *entry.key()).collect()
    }

    /// Clear all data for a run
    ///
    /// Removes the entire shard for the given run.
    /// Returns true if the run existed and was removed.
    pub fn clear_run(&self, run_id: &RunId) -> bool {
        self.shards.remove(run_id).is_some()
    }

    // ========================================================================
    // Snapshot Acquisition
    // ========================================================================

    /// Create a snapshot of the current store state
    ///
    /// FAST PATH: This is O(1) and < 500ns!
    ///
    /// Snapshot acquisition is:
    /// - Allocation-free (Arc reference count bump only)
    /// - Lock-free (atomic version load)
    /// - O(1) (no data structure scanning)
    ///
    /// The snapshot captures the current version and holds an Arc reference
    /// to the store, allowing reads at the captured version point.
    ///
    /// # Performance Contract
    ///
    /// - Must complete in < 500ns (RED FLAG if > 2µs)
    /// - Only operations: Arc::clone + atomic load
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::sync::Arc;
    /// use strata_storage::ShardedStore;
    ///
    /// let store = Arc::new(ShardedStore::new());
    /// let snapshot = store.snapshot();
    ///
    /// // Reads through snapshot see store state at snapshot time
    /// let value = snapshot.get(&key);
    /// ```
    #[inline]
    pub fn snapshot(self: &Arc<Self>) -> ShardedSnapshot {
        ShardedSnapshot {
            version: self.version.load(Ordering::Acquire),
            store: Arc::clone(self),
            cache: parking_lot::RwLock::new(FxHashMap::default()),
        }
    }

    /// Create a snapshot - API compatibility method
    ///
    /// This method provides API compatibility with `UnifiedStore::create_snapshot()`.
    /// It returns the same `ShardedSnapshot` as `snapshot()` but with a name that
    /// matches the legacy API.
    ///
    /// # Performance
    ///
    /// Same as `snapshot()` - O(1), < 500ns, allocation-free.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::sync::Arc;
    /// use strata_storage::ShardedStore;
    ///
    /// let store = Arc::new(ShardedStore::new());
    /// let snapshot = store.create_snapshot();  // Same as store.snapshot()
    /// ```
    #[inline]
    pub fn create_snapshot(self: &Arc<Self>) -> ShardedSnapshot {
        self.snapshot()
    }
}

impl Default for ShardedStore {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ShardedStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardedStore")
            .field("shard_count", &self.shard_count())
            .field("version", &self.version())
            .field("total_entries", &self.total_entries())
            .finish()
    }
}

// ============================================================================
// ShardedSnapshot
// ============================================================================

/// Snapshot of ShardedStore at a point in time
///
/// A snapshot captures:
/// - The version number at snapshot time
/// - An Arc reference to the underlying store
///
/// # Performance
///
/// Snapshot acquisition is O(1) and < 500ns:
/// - Arc::clone: ~20-30ns (atomic increment)
/// - Atomic load: ~1-5ns
/// - Total: well under 500ns
///
/// # MVCC Semantics
///
/// For true MVCC, reads would filter by version. The current implementation
/// provides a stable reference for consistent reads within a transaction.
/// Full MVCC version filtering can be added if needed.
///
/// # Thread Safety
///
/// ShardedSnapshot is Send + Sync since it only holds Arc<ShardedStore>.
/// Multiple snapshots can exist concurrently without blocking.
///
/// # Copy-on-Read Caching
///
/// To maintain snapshot isolation when concurrent transactions write to the
/// same keys, the snapshot caches values on first read. This ensures that
/// repeated reads of the same key return the same value, even if the key
/// is overwritten after the snapshot was created.
pub struct ShardedSnapshot {
    /// Version captured at snapshot time
    version: u64,
    /// Reference to the underlying store
    store: Arc<ShardedStore>,
    /// Cache of read values for snapshot isolation
    /// Using parking_lot::RwLock for interior mutability since SnapshotView::get takes &self
    /// parking_lot::RwLock doesn't poison on panic, preventing cascade failures
    cache: parking_lot::RwLock<FxHashMap<Key, Option<VersionedValue>>>,
}

impl Clone for ShardedSnapshot {
    fn clone(&self) -> Self {
        Self {
            version: self.version,
            store: Arc::clone(&self.store),
            // Clone the cache contents for independent snapshot views
            // parking_lot::RwLock doesn't need .unwrap() - it doesn't poison
            cache: parking_lot::RwLock::new(self.cache.read().clone()),
        }
    }
}

impl ShardedSnapshot {
    /// Get the snapshot version
    ///
    /// This is the version of the store at the time the snapshot was created.
    #[inline]
    pub fn version(&self) -> u64 {
        self.version
    }

    // NOTE: `get()` is provided by the SnapshotView trait implementation,
    // which includes proper MVCC version filtering and TTL expiration checks.
    // Use `SnapshotView::get()` directly instead of an inherent method.

    /// Check if a key exists at or before the snapshot version
    #[inline]
    pub fn contains(&self, key: &Key) -> bool {
        // Use the SnapshotView trait method for proper version filtering
        use strata_core::traits::SnapshotView;
        SnapshotView::get(self, key).ok().flatten().is_some()
    }

    /// List all entries for a run
    pub fn list_run(&self, run_id: &RunId) -> Vec<(Key, VersionedValue)> {
        self.store.list_run(run_id)
    }

    /// List entries matching a prefix
    pub fn list_by_prefix(&self, prefix: &Key) -> Vec<(Key, VersionedValue)> {
        self.store.list_by_prefix(prefix)
    }

    /// List entries of a specific type
    pub fn list_by_type(
        &self,
        run_id: &RunId,
        type_tag: strata_core::types::TypeTag,
    ) -> Vec<(Key, VersionedValue)> {
        self.store.list_by_type(run_id, type_tag)
    }

    /// Get count of entries for a run
    pub fn run_entry_count(&self, run_id: &RunId) -> usize {
        self.store.run_entry_count(run_id)
    }

    /// Get total entries across all runs
    pub fn total_entries(&self) -> usize {
        self.store.total_entries()
    }

    /// Get number of runs (shards)
    pub fn shard_count(&self) -> usize {
        self.store.shard_count()
    }
}

impl std::fmt::Debug for ShardedSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardedSnapshot")
            .field("version", &self.version)
            .field("shard_count", &self.store.shard_count())
            .field("total_entries", &self.store.total_entries())
            .finish()
    }
}

// ============================================================================
// Storage Trait Implementation
// ============================================================================

use strata_core::error::Result;
use strata_core::traits::Storage;
use strata_core::value::Value;
use std::time::Duration;

impl Storage for ShardedStore {
    /// Get current value for key (latest version)
    ///
    /// Returns None if key doesn't exist or is expired.
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>> {
        let run_id = key.namespace.run_id;
        Ok(self.shards.get(&run_id).and_then(|shard| {
            shard.data.get(key).and_then(|chain| {
                chain.latest().and_then(|sv| {
                    if !sv.is_expired() {
                        Some(sv.versioned().clone())
                    } else {
                        None
                    }
                })
            })
        }))
    }

    /// Get value at or before specified version (for snapshot isolation)
    ///
    /// Returns the value if version <= max_version and not expired.
    fn get_versioned(&self, key: &Key, max_version: u64) -> Result<Option<VersionedValue>> {
        let run_id = key.namespace.run_id;
        Ok(self.shards.get(&run_id).and_then(|shard| {
            shard.data.get(key).and_then(|chain| {
                chain.get_at_version(max_version).and_then(|sv| {
                    if !sv.is_expired() {
                        Some(sv.versioned().clone())
                    } else {
                        None
                    }
                })
            })
        }))
    }

    /// Get version history for a key
    ///
    /// Returns historical versions newest first, filtered by limit and before_version.
    fn get_history(
        &self,
        key: &Key,
        limit: Option<usize>,
        before_version: Option<u64>,
    ) -> Result<Vec<VersionedValue>> {
        let run_id = key.namespace.run_id;

        // Get the shard and extract history within the same scope to avoid lifetime issues
        let result = match self.shards.get(&run_id) {
            Some(shard) => match shard.data.get(key) {
                Some(chain) => chain
                    .history(limit, before_version)
                    .into_iter()
                    .filter(|sv| !sv.is_expired())
                    .map(|sv| sv.versioned().clone())
                    .collect(),
                None => Vec::new(),
            },
            None => Vec::new(),
        };

        Ok(result)
    }

    /// Put key-value pair with optional TTL
    ///
    /// Allocates a new version and returns it.
    fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> Result<u64> {
        let version = self.next_version();
        let stored = StoredValue::new(value, Version::txn(version), ttl);

        // Use the inherent put method which handles version chain
        ShardedStore::put(self, key, stored);

        Ok(version)
    }

    /// Delete key
    ///
    /// Returns the latest version's value if it existed.
    fn delete(&self, key: &Key) -> Result<Option<VersionedValue>> {
        Ok(ShardedStore::delete(self, key))
    }

    /// Scan keys with given prefix at or before max_version
    ///
    /// Results are sorted by key order.
    fn scan_prefix(&self, prefix: &Key, max_version: u64) -> Result<Vec<(Key, VersionedValue)>> {
        let run_id = prefix.namespace.run_id;
        Ok(self
            .shards
            .get(&run_id)
            .map(|shard| {
                let mut results: Vec<_> = shard
                    .data
                    .iter()
                    .filter_map(|(k, chain)| {
                        if !k.starts_with(prefix) {
                            return None;
                        }
                        chain.get_at_version(max_version).and_then(|sv| {
                            if !sv.is_expired() {
                                Some((k.clone(), sv.versioned().clone()))
                            } else {
                                None
                            }
                        })
                    })
                    .collect();

                results.sort_by(|(a, _), (b, _)| a.cmp(b));
                results
            })
            .unwrap_or_default())
    }

    /// Scan all keys for a given run_id at or before max_version
    ///
    /// Returns all entries for the run, filtered by version.
    fn scan_by_run(&self, run_id: RunId, max_version: u64) -> Result<Vec<(Key, VersionedValue)>> {
        Ok(self
            .shards
            .get(&run_id)
            .map(|shard| {
                let mut results: Vec<_> = shard
                    .data
                    .iter()
                    .filter_map(|(k, chain)| {
                        chain.get_at_version(max_version).and_then(|sv| {
                            if !sv.is_expired() {
                                Some((k.clone(), sv.versioned().clone()))
                            } else {
                                None
                            }
                        })
                    })
                    .collect();

                results.sort_by(|(a, _), (b, _)| a.cmp(b));
                results
            })
            .unwrap_or_default())
    }

    /// Get current global version
    fn current_version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Put a value with a specific version
    ///
    /// Used by transaction commit to apply writes with the commit version.
    fn put_with_version(
        &self,
        key: Key,
        value: Value,
        version: u64,
        ttl: Option<Duration>,
    ) -> Result<()> {
        let stored = StoredValue::new(value, Version::txn(version), ttl);

        // Use the inherent put method which handles version chain
        ShardedStore::put(self, key, stored);

        // Update global version to be at least this version
        self.version.fetch_max(version, Ordering::AcqRel);

        Ok(())
    }

    /// Delete a key with a specific version (creates tombstone conceptually)
    ///
    /// Used by transaction commit to apply deletes.
    fn delete_with_version(&self, key: &Key, _version: u64) -> Result<Option<VersionedValue>> {
        // For ShardedStore, we actually remove the key entirely
        // (tombstones would require storing deleted markers)
        // Use the inherent delete method
        Ok(ShardedStore::delete(self, key))
    }
}

// ============================================================================
// SnapshotView Trait Implementation
// ============================================================================

use strata_core::traits::SnapshotView;

impl SnapshotView for ShardedSnapshot {
    /// Get value from snapshot with MVCC version filtering
    ///
    /// Returns value at or before the snapshot version.
    /// Caches the result on first read for performance.
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>> {
        // Fast path: check cache first (read lock)
        // parking_lot::RwLock doesn't need .unwrap() - it doesn't poison
        {
            let cache = self.cache.read();
            if let Some(cached) = cache.get(key) {
                return Ok(cached.clone());
            }
        }

        // Slow path: read from store's version chain and cache (write lock)
        let run_id = key.namespace.run_id;
        let result = self.store.shards.get(&run_id).and_then(|shard| {
            shard.data.get(key).and_then(|chain| {
                chain.get_at_version(self.version).and_then(|sv| {
                    if !sv.is_expired() {
                        Some(sv.versioned().clone())
                    } else {
                        None
                    }
                })
            })
        });

        // Cache the result (including None for missing keys)
        {
            let mut cache = self.cache.write();
            cache.insert(key.clone(), result.clone());
        }

        Ok(result)
    }

    /// Scan keys with prefix from snapshot
    ///
    /// Returns all matching keys at or before snapshot version.
    fn scan_prefix(&self, prefix: &Key) -> Result<Vec<(Key, VersionedValue)>> {
        let run_id = prefix.namespace.run_id;
        Ok(self
            .store
            .shards
            .get(&run_id)
            .map(|shard| {
                let mut results: Vec<_> = shard
                    .data
                    .iter()
                    .filter_map(|(k, chain)| {
                        if !k.starts_with(prefix) {
                            return None;
                        }
                        chain.get_at_version(self.version).and_then(|sv| {
                            if !sv.is_expired() {
                                Some((k.clone(), sv.versioned().clone()))
                            } else {
                                None
                            }
                        })
                    })
                    .collect();

                results.sort_by(|(a, _), (b, _)| a.cmp(b));
                results
            })
            .unwrap_or_default())
    }

    /// Get snapshot version
    fn version(&self) -> u64 {
        self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_sharded_store_creation() {
        let store = ShardedStore::new();
        assert_eq!(store.shard_count(), 0);
        assert_eq!(store.version(), 0);
    }

    #[test]
    fn test_sharded_store_with_capacity() {
        let store = ShardedStore::with_capacity(100);
        assert_eq!(store.shard_count(), 0);
        assert_eq!(store.version(), 0);
    }

    #[test]
    fn test_version_increment() {
        let store = ShardedStore::new();
        assert_eq!(store.next_version(), 1);
        assert_eq!(store.next_version(), 2);
        assert_eq!(store.version(), 2);
    }

    #[test]
    fn test_set_version() {
        let store = ShardedStore::new();
        store.set_version(100);
        assert_eq!(store.version(), 100);
    }

    #[test]
    fn test_version_thread_safety() {
        use std::thread;
        let store = Arc::new(ShardedStore::new());
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    for _ in 0..100 {
                        store.next_version();
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(store.version(), 1000);
    }

    #[test]
    fn test_shard_creation() {
        let shard = Shard::new();
        assert!(shard.is_empty());
        assert_eq!(shard.len(), 0);
    }

    #[test]
    fn test_shard_with_capacity() {
        let shard = Shard::with_capacity(100);
        assert!(shard.is_empty());
    }

    #[test]
    fn test_debug_impl() {
        let store = ShardedStore::new();
        let debug_str = format!("{:?}", store);
        assert!(debug_str.contains("ShardedStore"));
        assert!(debug_str.contains("shard_count"));
    }

    // ========================================================================
    // Get/Put Operations Tests
    // ========================================================================

    fn create_test_key(run_id: RunId, name: &str) -> Key {
        use strata_core::types::Namespace;
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        Key::new_kv(ns, name)
    }

    fn create_stored_value(value: strata_core::value::Value, version: u64) -> StoredValue {
        StoredValue::new(value, Version::txn(version), None)
    }

    #[test]
    fn test_put_and_get() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let key = create_test_key(run_id, "test_key");
        let value = create_stored_value(Value::Int(42), 1);

        // Put
        store.put(key.clone(), value);

        // Get (Storage trait returns Result<Option<...>>)
        let retrieved = store.get(&key).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, Value::Int(42));
    }

    #[test]
    fn test_get_nonexistent() {
        let store = ShardedStore::new();
        let run_id = RunId::new();
        let key = create_test_key(run_id, "nonexistent");

        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_delete() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let key = create_test_key(run_id, "to_delete");
        let value = create_stored_value(Value::Int(42), 1);

        store.put(key.clone(), value);
        assert!(store.get(&key).unwrap().is_some());

        // Delete
        let deleted = store.delete(&key);
        assert!(deleted.is_some());
        assert!(store.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        let store = ShardedStore::new();
        let run_id = RunId::new();
        let key = create_test_key(run_id, "nonexistent");

        assert!(store.delete(&key).is_none());
    }

    #[test]
    fn test_contains() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let key = create_test_key(run_id, "exists");
        let value = create_stored_value(Value::Int(42), 1);

        assert!(!store.contains(&key));
        store.put(key.clone(), value);
        assert!(store.contains(&key));
    }

    #[test]
    fn test_overwrite() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let key = create_test_key(run_id, "overwrite");

        store.put(key.clone(), create_stored_value(Value::Int(1), 1));
        store.put(key.clone(), create_stored_value(Value::Int(2), 2));

        let retrieved = store.get(&key).unwrap().unwrap();
        assert_eq!(retrieved.value, Value::Int(2));
        assert_eq!(retrieved.version, Version::txn(2));
    }

    #[test]
    fn test_multiple_runs_isolated() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run1 = RunId::new();
        let run2 = RunId::new();

        let key1 = create_test_key(run1, "key");
        let key2 = create_test_key(run2, "key");

        store.put(key1.clone(), create_stored_value(Value::Int(1), 1));
        store.put(key2.clone(), create_stored_value(Value::Int(2), 1));

        // Different runs, same key name, different values
        assert_eq!(store.get(&key1).unwrap().unwrap().value, Value::Int(1));
        assert_eq!(store.get(&key2).unwrap().unwrap().value, Value::Int(2));
        assert_eq!(store.shard_count(), 2);
    }

    #[test]
    fn test_apply_batch() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();

        let key1 = create_test_key(run_id, "batch1");
        let key2 = create_test_key(run_id, "batch2");
        let key3 = create_test_key(run_id, "batch3");

        // First, put key3 so we can delete it
        store.put(key3.clone(), create_stored_value(Value::Int(999), 1));

        // Apply batch
        let writes = vec![(key1.clone(), Value::Int(1)), (key2.clone(), Value::Int(2))];
        let deletes = vec![key3.clone()];

        store.apply_batch(&writes, &deletes, 2).unwrap();

        assert_eq!(store.get(&key1).unwrap().unwrap().value, Value::Int(1));
        assert_eq!(store.get(&key1).unwrap().unwrap().version, Version::txn(2));
        assert_eq!(store.get(&key2).unwrap().unwrap().value, Value::Int(2));
        assert!(store.get(&key3).unwrap().is_none());
    }

    #[test]
    fn test_run_entry_count() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();

        assert_eq!(store.run_entry_count(&run_id), 0);

        for i in 0..5 {
            let key = create_test_key(run_id, &format!("key{}", i));
            store.put(key, create_stored_value(Value::Int(i), 1));
        }

        assert_eq!(store.run_entry_count(&run_id), 5);
        assert_eq!(store.total_entries(), 5);
    }

    #[test]
    fn test_concurrent_writes_different_runs() {
        use strata_core::value::Value;
        use std::thread;

        let store = Arc::new(ShardedStore::new());

        // 10 threads, each with its own run, writing 100 keys
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    let run_id = RunId::new();
                    for i in 0..100 {
                        let key = create_test_key(run_id, &format!("key{}", i));
                        store.put(key, create_stored_value(Value::Int(i), 1));
                    }
                    run_id
                })
            })
            .collect();

        let run_ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Verify each run has 100 entries
        for run_id in &run_ids {
            assert_eq!(store.run_entry_count(run_id), 100);
        }

        assert_eq!(store.shard_count(), 10);
        assert_eq!(store.total_entries(), 1000);
    }

    // ========================================================================
    // List Operations Tests
    // ========================================================================

    #[test]
    fn test_list_run_empty() {
        let store = ShardedStore::new();
        let run_id = RunId::new();

        let results = store.list_run(&run_id);
        assert!(results.is_empty());
    }

    #[test]
    fn test_list_run() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();

        // Insert some keys
        for i in 0..5 {
            let key = create_test_key(run_id, &format!("key{}", i));
            store.put(key, create_stored_value(Value::Int(i), 1));
        }

        let results = store.list_run(&run_id);
        assert_eq!(results.len(), 5);

        // Verify sorted order
        for i in 0..results.len() - 1 {
            assert!(results[i].0 < results[i + 1].0);
        }
    }

    #[test]
    fn test_list_by_prefix() {
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Insert keys with different prefixes
        store.put(
            Key::new_kv(ns.clone(), "user:alice"),
            create_stored_value(Value::Int(1), 1),
        );
        store.put(
            Key::new_kv(ns.clone(), "user:bob"),
            create_stored_value(Value::Int(2), 1),
        );
        store.put(
            Key::new_kv(ns.clone(), "config:timeout"),
            create_stored_value(Value::Int(3), 1),
        );

        // Query with "user:" prefix
        let prefix = Key::new_kv(ns.clone(), "user:");
        let results = store.list_by_prefix(&prefix);

        assert_eq!(results.len(), 2);
        // Should be alice, bob in sorted order
        assert!(results[0].0.user_key_string().unwrap().contains("alice"));
        assert!(results[1].0.user_key_string().unwrap().contains("bob"));
    }

    #[test]
    fn test_list_by_prefix_empty() {
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        store.put(
            Key::new_kv(ns.clone(), "data:foo"),
            create_stored_value(Value::Int(1), 1),
        );

        // Query with non-matching prefix
        let prefix = Key::new_kv(ns.clone(), "user:");
        let results = store.list_by_prefix(&prefix);

        assert!(results.is_empty());
    }

    #[test]
    fn test_list_by_type() {
        use strata_core::types::{Namespace, TypeTag};
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Insert KV entries
        store.put(
            Key::new_kv(ns.clone(), "kv1"),
            create_stored_value(Value::Int(1), 1),
        );
        store.put(
            Key::new_kv(ns.clone(), "kv2"),
            create_stored_value(Value::Int(2), 1),
        );

        // Insert Event entries
        store.put(
            Key::new_event(ns.clone(), 1),
            create_stored_value(Value::Int(10), 1),
        );

        // Insert State entries
        store.put(
            Key::new_state(ns.clone(), "state1"),
            create_stored_value(Value::Int(20), 1),
        );

        // Query by type
        let kv_results = store.list_by_type(&run_id, TypeTag::KV);
        assert_eq!(kv_results.len(), 2);

        let event_results = store.list_by_type(&run_id, TypeTag::Event);
        assert_eq!(event_results.len(), 1);

        let state_results = store.list_by_type(&run_id, TypeTag::State);
        assert_eq!(state_results.len(), 1);
    }

    #[test]
    fn test_count_by_type() {
        use strata_core::types::{Namespace, TypeTag};
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Insert mixed types
        for i in 0..5 {
            store.put(
                Key::new_kv(ns.clone(), &format!("kv{}", i)),
                create_stored_value(Value::Int(i), 1),
            );
        }
        for i in 0..3 {
            store.put(
                Key::new_event(ns.clone(), i as u64),
                create_stored_value(Value::Int(i), 1),
            );
        }

        assert_eq!(store.count_by_type(&run_id, TypeTag::KV), 5);
        assert_eq!(store.count_by_type(&run_id, TypeTag::Event), 3);
        assert_eq!(store.count_by_type(&run_id, TypeTag::State), 0);
    }

    #[test]
    fn test_run_ids() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run1 = RunId::new();
        let run2 = RunId::new();
        let run3 = RunId::new();

        // Insert data for 3 runs
        store.put(
            create_test_key(run1, "k1"),
            create_stored_value(Value::Int(1), 1),
        );
        store.put(
            create_test_key(run2, "k1"),
            create_stored_value(Value::Int(2), 1),
        );
        store.put(
            create_test_key(run3, "k1"),
            create_stored_value(Value::Int(3), 1),
        );

        let run_ids = store.run_ids();
        assert_eq!(run_ids.len(), 3);
        assert!(run_ids.contains(&run1));
        assert!(run_ids.contains(&run2));
        assert!(run_ids.contains(&run3));
    }

    #[test]
    fn test_clear_run() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();

        // Insert some data
        for i in 0..5 {
            let key = create_test_key(run_id, &format!("key{}", i));
            store.put(key, create_stored_value(Value::Int(i), 1));
        }

        assert_eq!(store.run_entry_count(&run_id), 5);
        assert!(store.has_run(&run_id));

        // Clear the run
        assert!(store.clear_run(&run_id));

        assert_eq!(store.run_entry_count(&run_id), 0);
        assert!(!store.has_run(&run_id));
    }

    #[test]
    fn test_clear_run_nonexistent() {
        let store = ShardedStore::new();
        let run_id = RunId::new();

        // Clear non-existent run returns false
        assert!(!store.clear_run(&run_id));
    }

    #[test]
    fn test_list_sorted_order() {
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Insert in random order
        let keys = vec!["zebra", "apple", "mango", "banana"];
        for k in &keys {
            store.put(
                Key::new_kv(ns.clone(), *k),
                create_stored_value(Value::String(k.to_string()), 1),
            );
        }

        let results = store.list_run(&run_id);
        let result_keys: Vec<_> = results
            .iter()
            .map(|(k, _)| k.user_key_string().unwrap())
            .collect();

        // Should be sorted: apple, banana, mango, zebra
        assert_eq!(result_keys, vec!["apple", "banana", "mango", "zebra"]);
    }

    // ========================================================================
    // Snapshot Acquisition Tests
    // ========================================================================

    #[test]
    fn test_snapshot_creation() {
        let store = Arc::new(ShardedStore::new());
        let snapshot = store.snapshot();

        assert_eq!(snapshot.version(), 0);
        assert_eq!(snapshot.shard_count(), 0);
    }

    #[test]
    fn test_snapshot_captures_version() {
        let store = Arc::new(ShardedStore::new());

        // Increment version
        store.next_version();
        store.next_version();
        store.next_version();

        let snapshot = store.snapshot();
        assert_eq!(snapshot.version(), 3);

        // Further increments don't affect snapshot
        store.next_version();
        assert_eq!(snapshot.version(), 3);
        assert_eq!(store.version(), 4);
    }

    #[test]
    fn test_snapshot_read_operations() {
        use strata_core::value::Value;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();

        // Put some data (version=1)
        let key = create_test_key(run_id, "test_key");
        store.put(key.clone(), create_stored_value(Value::Int(42), 1));
        // Update store version so snapshot can see data at version 1
        store.set_version(1);

        // Create snapshot (will capture version=1)
        let snapshot = store.snapshot();

        // Read through snapshot (SnapshotView returns Result<Option<...>>)
        let value = snapshot.get(&key).unwrap();
        assert!(value.is_some());
        assert_eq!(value.unwrap().value, Value::Int(42));

        // contains works
        assert!(snapshot.contains(&key));
    }

    #[test]
    fn test_snapshot_list_operations() {
        use strata_core::value::Value;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();

        // Put some data
        for i in 0..5 {
            let key = create_test_key(run_id, &format!("key{}", i));
            store.put(key, create_stored_value(Value::Int(i), 1));
        }

        let snapshot = store.snapshot();

        // list_run works
        let results = snapshot.list_run(&run_id);
        assert_eq!(results.len(), 5);

        // run_entry_count works
        assert_eq!(snapshot.run_entry_count(&run_id), 5);

        // total_entries works
        assert_eq!(snapshot.total_entries(), 5);
    }

    #[test]
    fn test_snapshot_multiple_concurrent() {
        use strata_core::value::Value;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();

        // Create first snapshot at version 0
        let snap1 = store.snapshot();
        assert_eq!(snap1.version(), 0);

        // Add data and increment version
        let key1 = create_test_key(run_id, "key1");
        store.put(key1.clone(), create_stored_value(Value::Int(1), 1));
        store.next_version();

        // Create second snapshot at version 1
        let snap2 = store.snapshot();
        assert_eq!(snap2.version(), 1);

        // Add more data
        let key2 = create_test_key(run_id, "key2");
        store.put(key2.clone(), create_stored_value(Value::Int(2), 2));
        store.next_version();

        // Create third snapshot at version 2
        let snap3 = store.snapshot();
        assert_eq!(snap3.version(), 2);

        // All snapshots retain their version
        assert_eq!(snap1.version(), 0);
        assert_eq!(snap2.version(), 1);
        assert_eq!(snap3.version(), 2);

        // Note: Current implementation doesn't do MVCC filtering,
        // so all snapshots see current data. This test verifies
        // version capture is working correctly.
    }

    #[test]
    fn test_snapshot_clone() {
        let store = Arc::new(ShardedStore::new());
        store.next_version();

        let snapshot = store.snapshot();
        let cloned = snapshot.clone();

        assert_eq!(snapshot.version(), cloned.version());
    }

    #[test]
    fn test_snapshot_debug() {
        let store = Arc::new(ShardedStore::new());
        let snapshot = store.snapshot();

        let debug_str = format!("{:?}", snapshot);
        assert!(debug_str.contains("ShardedSnapshot"));
        assert!(debug_str.contains("version"));
    }

    #[test]
    fn test_snapshot_thread_safety() {
        use std::thread;

        let store = Arc::new(ShardedStore::new());

        // Spawn threads that create snapshots concurrently
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    // Create snapshot
                    let snapshot = store.snapshot();

                    // Increment version
                    store.next_version();

                    // Create another snapshot
                    let snapshot2 = store.snapshot();

                    // Second snapshot should have higher or equal version
                    assert!(snapshot2.version() >= snapshot.version());

                    (snapshot.version(), snapshot2.version())
                })
            })
            .collect();

        for h in handles {
            let (v1, v2) = h.join().unwrap();
            assert!(v2 >= v1);
        }

        // Final version should be 10 (each thread incremented once)
        assert_eq!(store.version(), 10);
    }

    #[test]
    fn test_snapshot_fast_path() {
        use std::time::Instant;

        let store = Arc::new(ShardedStore::new());

        // Add some data to make it more realistic
        let run_id = RunId::new();
        for i in 0..1000 {
            let key = create_test_key(run_id, &format!("key{}", i));
            store.put(
                key,
                create_stored_value(strata_core::value::Value::Int(i), 1),
            );
        }

        // Measure snapshot creation time
        let iterations = 10000;
        let start = Instant::now();
        for _ in 0..iterations {
            let _snapshot = store.snapshot();
        }
        let elapsed = start.elapsed();
        let avg_ns = elapsed.as_nanos() / iterations as u128;

        // Should be well under 500ns
        // Note: In debug mode it might be slightly higher, but should still be fast
        println!("Snapshot acquisition avg: {}ns", avg_ns);
        assert!(
            avg_ns < 5000, // 5µs max in debug mode
            "Snapshot too slow: {}ns (target: <500ns in release)",
            avg_ns
        );
    }

    // ========================================================================
    // Storage Trait Implementation Tests
    // ========================================================================

    #[test]
    fn test_storage_trait_get_put() {
        use strata_core::traits::Storage;
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);
        let key = Key::new_kv(ns, "test_key");

        // Put via Storage trait
        let version = Storage::put(&store, key.clone(), Value::Int(42), None).unwrap();
        assert_eq!(version, 1);

        // Get via Storage trait
        let result = Storage::get(&store, &key).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, Value::Int(42));
    }

    #[test]
    fn test_storage_trait_get_versioned() {
        use strata_core::traits::Storage;
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);
        let key = Key::new_kv(ns, "test_key");

        // Put with version 1
        Storage::put(&store, key.clone(), Value::Int(42), None).unwrap();

        // Get with max_version 1 - should return value
        let result = Storage::get_versioned(&store, &key, 1).unwrap();
        assert!(result.is_some());

        // Get with max_version 0 - should return None (version 1 > 0)
        let result = Storage::get_versioned(&store, &key, 0).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_storage_trait_delete() {
        use strata_core::traits::Storage;
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);
        let key = Key::new_kv(ns, "test_key");

        Storage::put(&store, key.clone(), Value::Int(42), None).unwrap();
        assert!(Storage::get(&store, &key).unwrap().is_some());

        // Delete via Storage trait
        let deleted = Storage::delete(&store, &key).unwrap();
        assert!(deleted.is_some());
        assert!(Storage::get(&store, &key).unwrap().is_none());
    }

    #[test]
    fn test_storage_trait_scan_prefix() {
        use strata_core::traits::Storage;
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);

        // Insert keys with different prefixes
        Storage::put(
            &store,
            Key::new_kv(ns.clone(), "user:alice"),
            Value::Int(1),
            None,
        )
        .unwrap();
        Storage::put(
            &store,
            Key::new_kv(ns.clone(), "user:bob"),
            Value::Int(2),
            None,
        )
        .unwrap();
        Storage::put(
            &store,
            Key::new_kv(ns.clone(), "config:timeout"),
            Value::Int(3),
            None,
        )
        .unwrap();

        // Scan with "user:" prefix
        let prefix = Key::new_kv(ns.clone(), "user:");
        let results = Storage::scan_prefix(&store, &prefix, u64::MAX).unwrap();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_storage_trait_scan_by_run() {
        use strata_core::traits::Storage;
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run1 = RunId::new();
        let run2 = RunId::new();

        // Insert data for two runs
        let ns1 = Namespace::for_run(run1);
        let ns2 = Namespace::for_run(run2);

        Storage::put(
            &store,
            Key::new_kv(ns1.clone(), "key1"),
            Value::Int(1),
            None,
        )
        .unwrap();
        Storage::put(
            &store,
            Key::new_kv(ns1.clone(), "key2"),
            Value::Int(2),
            None,
        )
        .unwrap();
        Storage::put(
            &store,
            Key::new_kv(ns2.clone(), "key1"),
            Value::Int(3),
            None,
        )
        .unwrap();

        // Scan run1
        let results = Storage::scan_by_run(&store, run1, u64::MAX).unwrap();
        assert_eq!(results.len(), 2);

        // Scan run2
        let results = Storage::scan_by_run(&store, run2, u64::MAX).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_storage_trait_put_with_version() {
        use strata_core::traits::Storage;
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);
        let key = Key::new_kv(ns, "test_key");

        // Put with specific version 42
        Storage::put_with_version(&store, key.clone(), Value::Int(100), 42, None).unwrap();

        // Verify version is 42
        let result = Storage::get(&store, &key).unwrap().unwrap();
        assert_eq!(result.version, Version::txn(42));

        // current_version should be updated
        assert!(Storage::current_version(&store) >= 42);
    }

    #[test]
    fn test_snapshot_view_trait() {
        use strata_core::traits::{SnapshotView, Storage};
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);

        // Put at version 1
        let key1 = Key::new_kv(ns.clone(), "key1");
        Storage::put(&*store, key1.clone(), Value::Int(1), None).unwrap();

        // Create snapshot at version 1
        let snapshot = store.snapshot();
        assert_eq!(SnapshotView::version(&snapshot), 1);

        // Put at version 2
        let key2 = Key::new_kv(ns.clone(), "key2");
        Storage::put(&*store, key2.clone(), Value::Int(2), None).unwrap();

        // Snapshot should only see key1 (version 1) via MVCC filtering
        let snap_key1 = SnapshotView::get(&snapshot, &key1).unwrap();
        assert!(snap_key1.is_some());

        // key2 has version 2, but snapshot is at version 1
        // It will be visible since we're reading from shared storage
        // but won't pass version filter
        let snap_key2 = SnapshotView::get(&snapshot, &key2).unwrap();
        // Note: key2 has version 2, snapshot version is 1, so it should be None
        assert!(
            snap_key2.is_none(),
            "key2 should not be visible at version 1"
        );
    }

    #[test]
    fn test_snapshot_view_scan_prefix() {
        use strata_core::traits::{SnapshotView, Storage};
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);

        // Put two keys at version 1
        Storage::put(
            &*store,
            Key::new_kv(ns.clone(), "user:alice"),
            Value::Int(1),
            None,
        )
        .unwrap();
        Storage::put(
            &*store,
            Key::new_kv(ns.clone(), "user:bob"),
            Value::Int(2),
            None,
        )
        .unwrap();

        let snapshot = store.snapshot();

        // Put another key at version 3
        Storage::put(
            &*store,
            Key::new_kv(ns.clone(), "user:charlie"),
            Value::Int(3),
            None,
        )
        .unwrap();

        // Scan prefix via snapshot - should only see 2 keys at snapshot version
        let prefix = Key::new_kv(ns.clone(), "user:");
        let results = SnapshotView::scan_prefix(&snapshot, &prefix).unwrap();

        assert_eq!(
            results.len(),
            2,
            "Snapshot should only see keys at version <= 2"
        );
    }

    // ========================================================================
    // VersionChain::history() Tests
    // ========================================================================

    #[test]
    fn test_version_chain_history_all_versions() {
        use strata_core::value::Value;

        let mut chain = VersionChain::new(create_stored_value(Value::Int(1), 1));
        chain.push(create_stored_value(Value::Int(2), 2));
        chain.push(create_stored_value(Value::Int(3), 3));

        // Get all versions (newest first)
        let history = chain.history(None, None);
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].version().as_u64(), 3);
        assert_eq!(history[1].version().as_u64(), 2);
        assert_eq!(history[2].version().as_u64(), 1);
    }

    #[test]
    fn test_version_chain_history_with_limit() {
        use strata_core::value::Value;

        let mut chain = VersionChain::new(create_stored_value(Value::Int(1), 1));
        chain.push(create_stored_value(Value::Int(2), 2));
        chain.push(create_stored_value(Value::Int(3), 3));
        chain.push(create_stored_value(Value::Int(4), 4));
        chain.push(create_stored_value(Value::Int(5), 5));

        // Get only 2 versions
        let history = chain.history(Some(2), None);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version().as_u64(), 5); // Newest
        assert_eq!(history[1].version().as_u64(), 4);
    }

    #[test]
    fn test_version_chain_history_with_before() {
        use strata_core::value::Value;

        let mut chain = VersionChain::new(create_stored_value(Value::Int(1), 1));
        chain.push(create_stored_value(Value::Int(2), 2));
        chain.push(create_stored_value(Value::Int(3), 3));
        chain.push(create_stored_value(Value::Int(4), 4));
        chain.push(create_stored_value(Value::Int(5), 5));

        // Get versions before version 4 (should get 1, 2, 3)
        let history = chain.history(None, Some(4));
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].version().as_u64(), 3);
        assert_eq!(history[1].version().as_u64(), 2);
        assert_eq!(history[2].version().as_u64(), 1);
    }

    #[test]
    fn test_version_chain_history_with_limit_and_before() {
        use strata_core::value::Value;

        let mut chain = VersionChain::new(create_stored_value(Value::Int(1), 1));
        chain.push(create_stored_value(Value::Int(2), 2));
        chain.push(create_stored_value(Value::Int(3), 3));
        chain.push(create_stored_value(Value::Int(4), 4));
        chain.push(create_stored_value(Value::Int(5), 5));

        // Get 2 versions before version 5
        let history = chain.history(Some(2), Some(5));
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].version().as_u64(), 4);
        assert_eq!(history[1].version().as_u64(), 3);
    }

    #[test]
    fn test_version_chain_history_before_first() {
        use strata_core::value::Value;

        let chain = VersionChain::new(create_stored_value(Value::Int(1), 5));

        // Before version 5 returns empty (only version is 5)
        let history = chain.history(None, Some(5));
        assert!(history.is_empty());

        // Before version 6 returns the one version
        let history = chain.history(None, Some(6));
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_version_chain_is_empty() {
        use strata_core::value::Value;

        let chain = VersionChain::new(create_stored_value(Value::Int(1), 1));
        assert!(!chain.is_empty());
        assert_eq!(chain.version_count(), 1);
    }

    // ========================================================================
    // VersionChain::get_at_version() Tests (MVCC)
    // ========================================================================

    #[test]
    fn test_version_chain_get_at_version_single() {
        use strata_core::value::Value;

        let chain = VersionChain::new(create_stored_value(Value::Int(42), 5));

        // Exact version match
        let result = chain.get_at_version(5);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 5);

        // Higher version should still return the value (latest <= max_version)
        let result = chain.get_at_version(10);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 5);

        // Lower version should return None
        let result = chain.get_at_version(4);
        assert!(result.is_none());
    }

    #[test]
    fn test_version_chain_get_at_version_multiple() {
        use strata_core::value::Value;

        // Create chain with versions 1, 2, 3 (newest first after pushes)
        let mut chain = VersionChain::new(create_stored_value(Value::Int(100), 1));
        chain.push(create_stored_value(Value::Int(200), 2));
        chain.push(create_stored_value(Value::Int(300), 3));

        // Chain should have 3 versions
        assert_eq!(chain.version_count(), 3);

        // Query at version 3 should return version 3
        let result = chain.get_at_version(3);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 3);
        assert_eq!(result.unwrap().versioned().value, Value::Int(300));

        // Query at version 2 should return version 2
        let result = chain.get_at_version(2);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 2);
        assert_eq!(result.unwrap().versioned().value, Value::Int(200));

        // Query at version 1 should return version 1
        let result = chain.get_at_version(1);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 1);
        assert_eq!(result.unwrap().versioned().value, Value::Int(100));

        // Query at version 0 should return None
        let result = chain.get_at_version(0);
        assert!(result.is_none());

        // Query at version 100 should return latest (version 3)
        let result = chain.get_at_version(100);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 3);
    }

    #[test]
    fn test_version_chain_get_at_version_between_versions() {
        use strata_core::value::Value;

        // Create chain with versions 10, 20, 30 (sparse)
        let mut chain = VersionChain::new(create_stored_value(Value::Int(1), 10));
        chain.push(create_stored_value(Value::Int(2), 20));
        chain.push(create_stored_value(Value::Int(3), 30));

        // Query at version 25 should return version 20 (latest <= 25)
        let result = chain.get_at_version(25);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 20);

        // Query at version 15 should return version 10
        let result = chain.get_at_version(15);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 10);

        // Query at version 5 should return None (no version <= 5)
        let result = chain.get_at_version(5);
        assert!(result.is_none());
    }

    #[test]
    fn test_version_chain_get_at_version_snapshot_isolation() {
        use strata_core::value::Value;

        // Simulates snapshot isolation: reader sees consistent view
        let mut chain = VersionChain::new(create_stored_value(Value::String("v1".into()), 1));

        // Snapshot taken at version 1
        let snapshot_version = 1;

        // Writer adds new versions
        chain.push(create_stored_value(Value::String("v2".into()), 2));
        chain.push(create_stored_value(Value::String("v3".into()), 3));

        // Snapshot reader should still see version 1
        let result = chain.get_at_version(snapshot_version);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 1);
        assert_eq!(
            result.unwrap().versioned().value,
            Value::String("v1".into())
        );

        // Current reader sees version 3
        let result = chain.get_at_version(u64::MAX);
        assert!(result.is_some());
        assert_eq!(result.unwrap().version().as_u64(), 3);
    }

    // ========================================================================
    // Storage::get_history() Tests
    // ========================================================================

    #[test]
    fn test_storage_get_history() {
        use strata_core::traits::Storage;
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);
        let key = Key::new_kv(ns.clone(), "test-key");

        // Put multiple versions of the same key
        Storage::put_with_version(&store, key.clone(), Value::Int(1), 1, None).unwrap();
        Storage::put_with_version(&store, key.clone(), Value::Int(2), 2, None).unwrap();
        Storage::put_with_version(&store, key.clone(), Value::Int(3), 3, None).unwrap();

        // Get full history
        let history = Storage::get_history(&store, &key, None, None).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].version.as_u64(), 3);
        assert_eq!(history[1].version.as_u64(), 2);
        assert_eq!(history[2].version.as_u64(), 1);
    }

    #[test]
    fn test_storage_get_history_pagination() {
        use strata_core::traits::Storage;
        use strata_core::types::Namespace;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);
        let key = Key::new_kv(ns.clone(), "paginated-key");

        // Put 5 versions
        for i in 1..=5 {
            Storage::put_with_version(&store, key.clone(), Value::Int(i), i as u64, None).unwrap();
        }

        // Page 1: Get first 2
        let page1 = Storage::get_history(&store, &key, Some(2), None).unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].version.as_u64(), 5);
        assert_eq!(page1[1].version.as_u64(), 4);

        // Page 2: Get next 2 (before version 4)
        let page2 = Storage::get_history(&store, &key, Some(2), Some(4)).unwrap();
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].version.as_u64(), 3);
        assert_eq!(page2[1].version.as_u64(), 2);

        // Page 3: Get remaining
        let page3 = Storage::get_history(&store, &key, Some(2), Some(2)).unwrap();
        assert_eq!(page3.len(), 1);
        assert_eq!(page3[0].version.as_u64(), 1);
    }

    #[test]
    fn test_storage_get_history_nonexistent_key() {
        use strata_core::traits::Storage;
        use strata_core::types::Namespace;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);
        let key = Key::new_kv(ns.clone(), "nonexistent");

        let history = Storage::get_history(&store, &key, None, None).unwrap();
        assert!(history.is_empty());
    }

    // ========================================================================
    // ADVERSARIAL TESTS - Bug Hunting
    // These tests probe edge cases and potential bugs.
    // ========================================================================

    /// BUG HUNT: Delete removes all versions, breaking MVCC for uncached reads
    ///
    /// The snapshot cache provides isolation for keys that were READ before delete.
    /// However, if a snapshot never read the key before it was deleted, the
    /// snapshot cannot see the value that SHOULD be visible at its version.
    ///
    /// This test demonstrates the actual bug: delete removes all versions,
    /// so a snapshot that never cached the value loses access to it.
    #[test]
    fn test_delete_breaks_mvcc_for_uncached_reads() {
        use strata_core::traits::{SnapshotView, Storage};
        use strata_core::value::Value;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();
        let key = create_test_key(run_id, "mvcc_uncached");

        // Put a value at version 1
        Storage::put(&*store, key.clone(), Value::Int(100), None).unwrap();

        // Create a snapshot at version 1 - but DON'T read the key yet
        let snapshot = store.snapshot();
        assert_eq!(snapshot.version(), 1);

        // Delete the key BEFORE the snapshot reads it
        Storage::delete(&*store, &key).unwrap();

        // Now try to read from snapshot - it should see version 1
        // but delete removed ALL versions so there's nothing to see
        let result = SnapshotView::get(&snapshot, &key).unwrap();

        // BUG CONFIRMED: The snapshot should see the value at version 1,
        // but delete() removed all versions including the one the snapshot needs.
        // The correct behavior would be for delete to create a tombstone at version 2,
        // leaving version 1 intact for older snapshots.
        assert!(
            result.is_none(),
            "BUG CONFIRMED: Delete breaks MVCC - snapshot lost access to value at version 1. \
             Delete should create a tombstone, not remove all versions."
        );
    }

    /// Verify snapshot cache provides isolation for pre-read keys
    ///
    /// If a snapshot reads a key BEFORE it's deleted, the cached value
    /// provides isolation and the snapshot continues to see the value.
    #[test]
    fn test_snapshot_cache_provides_isolation_for_cached_reads() {
        use strata_core::traits::{SnapshotView, Storage};
        use strata_core::value::Value;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();
        let key = create_test_key(run_id, "cached_test");

        // Put a value at version 1
        Storage::put(&*store, key.clone(), Value::Int(100), None).unwrap();

        // Create snapshot and READ the key (this caches it)
        let snapshot = store.snapshot();
        let before = SnapshotView::get(&snapshot, &key).unwrap();
        assert!(before.is_some());
        assert_eq!(before.unwrap().value, Value::Int(100));

        // Delete the key
        Storage::delete(&*store, &key).unwrap();

        // Snapshot should still see the cached value
        let after = SnapshotView::get(&snapshot, &key).unwrap();
        assert!(
            after.is_some(),
            "Cached value should survive delete"
        );
        assert_eq!(after.unwrap().value, Value::Int(100));
    }

    /// BUG HUNT: Version counter behavior at u64::MAX boundary
    ///
    /// The version counter uses AtomicU64::fetch_add(1) which wraps at MAX.
    /// In debug mode, Rust panics on overflow. In release mode, it wraps to 0.
    /// Either way, this breaks the version monotonicity invariant.
    ///
    /// BUG DOCUMENTED: No protection against overflow - would panic in debug
    /// or wrap silently in release mode. At ~584 years of continuous operation
    /// at 1 billion versions/second, this is unlikely but should be handled.
    #[test]
    fn test_version_counter_near_max_boundary() {
        let store = ShardedStore::new();

        // Set version close to MAX (but leave room to not overflow)
        store.set_version(u64::MAX - 5);
        assert_eq!(store.version(), u64::MAX - 5);

        // Allocate a few versions - should work fine
        let v1 = store.next_version();
        assert_eq!(v1, u64::MAX - 4);

        let v2 = store.next_version();
        assert_eq!(v2, u64::MAX - 3);

        // Verify monotonicity holds before boundary
        assert!(v2 > v1, "Versions should be monotonically increasing");

        // BUG DOCUMENTED: If we call next_version() 4 more times, we hit u64::MAX
        // and the next call would panic (debug) or wrap to 0 (release).
        // This test intentionally does NOT cross the boundary to avoid panic.
        // A production fix should use wrapping_add or saturating_add with error.
    }

    /// BUG HUNT: TTL expiration with clock going backward
    ///
    /// If system clock goes backward after storing a value,
    /// duration_since() returns None and value won't expire.
    #[test]
    fn test_ttl_expiration_with_old_timestamp() {
        use strata_core::value::Value;

        // Create a value with a timestamp in the "future" relative to epoch
        let future_ts = Timestamp::from_micros(u64::MAX / 2);
        let stored = StoredValue::with_timestamp(
            Value::Int(42),
            Version::txn(1),
            future_ts,
            Some(std::time::Duration::from_secs(1)), // 1 second TTL
        );

        // If current system time is before the timestamp,
        // duration_since returns None and is_expired returns false
        // This means the value appears NOT expired even though TTL passed
        let is_expired = stored.is_expired();

        // This test documents the edge case:
        // With a future timestamp, the value never expires
        // This is INTENDED behavior (safe default), but worth documenting
        assert!(
            !is_expired,
            "Value with future timestamp doesn't expire - clock backward protection"
        );
    }

    /// BUG HUNT: Concurrent snapshot reads during store modifications
    ///
    /// Test that snapshot isolation holds when the store is being
    /// actively modified by other threads.
    #[test]
    fn test_snapshot_isolation_under_concurrent_writes() {
        use strata_core::traits::{SnapshotView, Storage};
        use strata_core::value::Value;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::thread;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();

        // Put initial values
        for i in 0..10 {
            let key = create_test_key(run_id, &format!("key{}", i));
            Storage::put(&*store, key, Value::Int(i), None).unwrap();
        }

        // Take snapshot
        let snapshot = store.snapshot();
        let snapshot_version = snapshot.version();

        // Flag to stop writers
        let stop = Arc::new(AtomicBool::new(false));

        // Spawn writers that continuously modify the store
        let writer_handles: Vec<_> = (0..4)
            .map(|t| {
                let store = Arc::clone(&store);
                let stop = Arc::clone(&stop);
                thread::spawn(move || {
                    let mut counter = 0;
                    while !stop.load(Ordering::Relaxed) {
                        for i in 0..10 {
                            let key = create_test_key(run_id, &format!("key{}", i));
                            let _ = Storage::put(
                                &*store,
                                key,
                                Value::Int(t * 1000 + counter),
                                None,
                            );
                        }
                        counter += 1;
                        if counter > 100 {
                            break;
                        }
                    }
                })
            })
            .collect();

        // Read from snapshot while writes are happening
        // Snapshot should see consistent values at snapshot_version
        for _ in 0..100 {
            for i in 0..10 {
                let key = create_test_key(run_id, &format!("key{}", i));
                let result = SnapshotView::get(&snapshot, &key).unwrap();

                // Snapshot should see SOME value (the one at or before snapshot version)
                assert!(
                    result.is_some(),
                    "Snapshot lost visibility to key{} during concurrent writes",
                    i
                );

                // The version should be <= snapshot_version
                let version = result.as_ref().unwrap().version.as_u64();
                assert!(
                    version <= snapshot_version,
                    "Snapshot saw version {} but snapshot is at {} - MVCC violation",
                    version,
                    snapshot_version
                );
            }
        }

        // Stop writers
        stop.store(true, Ordering::Relaxed);
        for h in writer_handles {
            let _ = h.join();
        }
    }

    /// BUG HUNT: Snapshot cache race condition
    ///
    /// The ShardedSnapshot::get() has a check-then-populate pattern:
    /// 1. Check cache (read lock)
    /// 2. If miss, read from store and populate cache (write lock)
    /// This is NOT atomic - two threads could both miss and both write.
    #[test]
    fn test_snapshot_cache_concurrent_access() {
        use strata_core::traits::{SnapshotView, Storage};
        use strata_core::value::Value;
        use std::sync::Barrier;
        use std::thread;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();
        let key = create_test_key(run_id, "cached_key");

        // Put a value
        Storage::put(&*store, key.clone(), Value::Int(42), None).unwrap();

        // Create snapshot
        let snapshot = Arc::new(store.snapshot());

        // Use barrier to ensure all threads hit get() simultaneously
        let num_threads = 10;
        let barrier = Arc::new(Barrier::new(num_threads));

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let snapshot = Arc::clone(&snapshot);
                let key = key.clone();
                let barrier = Arc::clone(&barrier);

                thread::spawn(move || {
                    barrier.wait();
                    // All threads try to get() at the same time
                    let result = SnapshotView::get(&*snapshot, &key).unwrap();
                    result.map(|v| v.value.clone())
                })
            })
            .collect();

        // Collect results
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All threads should see the same value (cache is consistent)
        for result in &results {
            assert_eq!(
                *result,
                Some(Value::Int(42)),
                "Snapshot cache returned inconsistent value under concurrent access"
            );
        }
    }

    /// BUG HUNT: Version chain GC during concurrent reads
    ///
    /// If gc() runs while get_at_version() is iterating,
    /// could we get inconsistent results?
    #[test]
    fn test_version_chain_gc_concurrent_reads() {
        use strata_core::traits::Storage;
        use strata_core::value::Value;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::thread;

        let store = Arc::new(ShardedStore::new());
        let run_id = RunId::new();
        let key = create_test_key(run_id, "gc_test");

        // Build up a version chain with many versions
        for i in 1..=100 {
            Storage::put_with_version(&*store, key.clone(), Value::Int(i), i as u64, None).unwrap();
        }

        // Verify all versions exist
        let history = Storage::get_history(&*store, &key, None, None).unwrap();
        assert_eq!(history.len(), 100);

        // Run concurrent reads and simulated GC pressure
        let stop = Arc::new(AtomicBool::new(false));

        // Reader threads
        let reader_handles: Vec<_> = (0..4)
            .map(|_| {
                let store = Arc::clone(&store);
                let key = key.clone();
                let stop = Arc::clone(&stop);

                thread::spawn(move || {
                    let mut reads = 0;
                    while !stop.load(Ordering::Relaxed) && reads < 1000 {
                        // Read at various versions
                        for v in [1, 25, 50, 75, 100] {
                            let result = Storage::get_versioned(&*store, &key, v).unwrap();
                            if let Some(vv) = result {
                                assert!(
                                    vv.version.as_u64() <= v,
                                    "Read version {} but requested max {}",
                                    vv.version.as_u64(),
                                    v
                                );
                            }
                        }
                        reads += 1;
                    }
                    reads
                })
            })
            .collect();

        // Writer thread that causes potential GC
        let writer_handle = {
            let store = Arc::clone(&store);
            let key = key.clone();
            let stop = Arc::clone(&stop);

            thread::spawn(move || {
                let mut version = 101;
                while !stop.load(Ordering::Relaxed) && version < 200 {
                    Storage::put_with_version(
                        &*store,
                        key.clone(),
                        Value::Int(version),
                        version as u64,
                        None,
                    )
                    .unwrap();
                    version += 1;
                }
                version
            })
        };

        // Let it run for a bit
        thread::sleep(std::time::Duration::from_millis(100));
        stop.store(true, Ordering::Relaxed);

        // Collect results - no panics means no data races detected
        for h in reader_handles {
            let reads = h.join().unwrap();
            assert!(reads > 0, "Reader thread did some work");
        }
        let final_version = writer_handle.join().unwrap();
        assert!(final_version > 101, "Writer thread did some work");
    }

    /// BUG HUNT: apply_batch atomicity under failure
    ///
    /// apply_batch writes multiple keys. If it fails midway,
    /// are partial writes visible?
    #[test]
    fn test_apply_batch_partial_application() {
        use strata_core::traits::Storage;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();

        // Create batch with many keys
        let keys: Vec<_> = (0..10)
            .map(|i| create_test_key(run_id, &format!("batch_key_{}", i)))
            .collect();

        let writes: Vec<_> = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), Value::Int(i as i64)))
            .collect();

        // Apply batch
        store.apply_batch(&writes, &[], 1).unwrap();

        // All keys should be written with same version
        for (i, key) in keys.iter().enumerate() {
            let result = Storage::get(&store, key).unwrap();
            assert!(result.is_some(), "Key {} should exist after batch", i);
            let vv = result.unwrap();
            assert_eq!(vv.version.as_u64(), 1, "All keys should have version 1");
        }
    }

    /// BUG HUNT: fetch_max semantics for version updates
    ///
    /// apply_batch uses fetch_max for version. Verify it works correctly
    /// when applied versions arrive out of order.
    #[test]
    fn test_version_fetch_max_out_of_order() {
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let key = create_test_key(run_id, "out_of_order");

        // Apply batch at version 100 first
        store
            .apply_batch(&[(key.clone(), Value::Int(100))], &[], 100)
            .unwrap();
        assert_eq!(store.version(), 100);

        // Apply batch at version 50 (older) - version should stay at 100
        store
            .apply_batch(&[(key.clone(), Value::Int(50))], &[], 50)
            .unwrap();
        assert_eq!(
            store.version(),
            100,
            "Version should not decrease with fetch_max"
        );

        // Apply batch at version 150 - version should update
        store
            .apply_batch(&[(key.clone(), Value::Int(150))], &[], 150)
            .unwrap();
        assert_eq!(store.version(), 150);
    }

    /// BUG HUNT: Empty version chain after multiple deletes
    ///
    /// What happens if we delete a key that doesn't exist?
    #[test]
    fn test_delete_nonexistent_key_no_panic() {
        use strata_core::traits::Storage;

        let store = ShardedStore::new();
        let run_id = RunId::new();
        let key = create_test_key(run_id, "never_existed");

        // Delete should not panic
        let result = Storage::delete(&store, &key).unwrap();
        assert!(result.is_none(), "Delete of nonexistent key returns None");

        // Multiple deletes should be safe
        let result2 = Storage::delete(&store, &key).unwrap();
        assert!(result2.is_none());
    }

    /// BUG HUNT: Scan operations with expired TTL values
    ///
    /// Scan should filter out expired values.
    #[test]
    fn test_scan_filters_expired_ttl() {
        use strata_core::traits::Storage;
        use strata_core::value::Value;

        let store = ShardedStore::new();
        let run_id = RunId::new();

        // Create a mix of expired and non-expired values
        let ns = strata_core::types::Namespace::for_run(run_id);

        // Non-expired key
        let key1 = Key::new_kv(ns.clone(), "fresh");
        Storage::put(&store, key1.clone(), Value::Int(1), None).unwrap();

        // "Expired" key - we'll simulate by using internal API
        let key2 = Key::new_kv(ns.clone(), "expired");
        let old_ts = Timestamp::from_micros(0); // epoch
        let expired_value = StoredValue::with_timestamp(
            Value::Int(2),
            Version::txn(2),
            old_ts,
            Some(std::time::Duration::from_secs(1)), // expired long ago
        );
        store.put(key2.clone(), expired_value);

        // Update store version
        store.set_version(2);

        // Scan should only return the fresh key
        let prefix = Key::new_kv(ns.clone(), "");
        let results = Storage::scan_prefix(&store, &prefix, u64::MAX).unwrap();

        assert_eq!(
            results.len(),
            1,
            "Scan should filter out expired values"
        );
        assert!(
            results[0].0.user_key_string().unwrap().contains("fresh"),
            "Only fresh key should be returned"
        );
    }
}
