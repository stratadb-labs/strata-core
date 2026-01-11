//! Transaction context for OCC
//!
//! This module implements the core transaction data structure for optimistic
//! concurrency control. TransactionContext tracks all reads, writes, deletes,
//! and CAS operations for a transaction, enabling validation at commit time.
//!
//! See `docs/architecture/M2_TRANSACTION_SEMANTICS.md` for the full specification.

use in_mem_core::error::{Error, Result};
use in_mem_core::traits::SnapshotView;
use in_mem_core::types::{Key, RunId};
use in_mem_core::value::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Status of a transaction in its lifecycle
///
/// State transitions:
/// - `Active` → `Validating` (begin commit)
/// - `Validating` → `Committed` (validation passed)
/// - `Validating` → `Aborted` (conflict detected)
/// - `Active` → `Aborted` (user abort or error)
///
/// Terminal states (no transitions allowed):
/// - `Committed`
/// - `Aborted`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction is executing, can read/write
    Active,
    /// Transaction is being validated for conflicts
    Validating,
    /// Transaction committed successfully
    Committed,
    /// Transaction was aborted
    Aborted {
        /// Human-readable reason for abort
        reason: String,
    },
}

/// A compare-and-swap operation to be validated at commit
///
/// CAS operations are buffered until commit time. At commit:
/// 1. Validate that the key's current version equals `expected_version`
/// 2. If valid, write `new_value`
/// 3. If invalid, abort the transaction
///
/// Note: CAS does NOT automatically add to read_set. If you want read-set
/// protection in addition to CAS validation, explicitly read the key first.
#[derive(Debug, Clone)]
pub struct CASOperation {
    /// Key to CAS
    pub key: Key,
    /// Expected version (0 = key must not exist)
    pub expected_version: u64,
    /// New value to write if version matches
    pub new_value: Value,
}

/// Transaction context for OCC with snapshot isolation
///
/// Tracks all reads, writes, deletes, and CAS operations for a transaction.
/// Validation and commit happen at transaction end.
///
/// # Read-Your-Writes Semantics
///
/// When reading a key, the transaction checks in order:
/// 1. **write_set**: Returns uncommitted write from this transaction
/// 2. **delete_set**: Returns None for uncommitted delete
/// 3. **snapshot**: Returns value from snapshot, tracks in read_set
///
/// # Read-Set Tracking
///
/// All reads from the snapshot are tracked in `read_set` with the version read.
/// At commit time, these versions are validated against current storage.
/// If any version changed, the transaction has a read-write conflict.
///
/// # Lifecycle
///
/// 1. **BEGIN**: Create with `with_snapshot()`, status is `Active`
/// 2. **READ/WRITE**: Use `get()`, `put()`, `delete()`, `cas()`
/// 3. **VALIDATE**: Call `mark_validating()`, check for conflicts
/// 4. **COMMIT/ABORT**: Call `mark_committed()` or `mark_aborted()`
pub struct TransactionContext {
    // Identity
    /// Unique transaction ID
    pub txn_id: u64,
    /// Run this transaction belongs to
    pub run_id: RunId,

    // Snapshot isolation
    /// Version at transaction start (snapshot version)
    ///
    /// All reads see data as of this version. Used for conflict detection.
    pub start_version: u64,

    /// Snapshot view for this transaction
    ///
    /// Provides consistent point-in-time view of storage.
    snapshot: Option<Box<dyn SnapshotView>>,

    // Operation tracking
    /// Keys read and their versions (for validation)
    ///
    /// At commit time, we check that each key's current version still matches
    /// the version we read. If not, there's a read-write conflict.
    ///
    /// Version 0 means the key did not exist when read.
    pub read_set: HashMap<Key, u64>,

    /// Keys written with their new values (buffered)
    ///
    /// These writes are not visible to other transactions until commit.
    /// At commit, they are applied atomically to storage.
    pub write_set: HashMap<Key, Value>,

    /// Keys to delete (buffered)
    ///
    /// Deletes are buffered like writes. A deleted key returns None
    /// when read within this transaction (read-your-deletes).
    pub delete_set: HashSet<Key>,

    /// CAS operations to validate and apply
    ///
    /// Each CAS is validated at commit time against the current storage
    /// version, independent of the read_set.
    pub cas_set: Vec<CASOperation>,

    // State
    /// Current transaction status
    pub status: TransactionStatus,
}

impl TransactionContext {
    /// Create a new transaction context without a snapshot
    ///
    /// This constructor is primarily for testing or for transactions
    /// that don't need to read from storage.
    ///
    /// For normal transactions, use `with_snapshot()`.
    ///
    /// # Arguments
    /// * `txn_id` - Unique transaction identifier
    /// * `run_id` - Run this transaction belongs to
    /// * `start_version` - Snapshot version at transaction start
    ///
    /// # Example
    ///
    /// ```
    /// use in_mem_concurrency::TransactionContext;
    /// use in_mem_core::types::RunId;
    ///
    /// let run_id = RunId::new();
    /// let txn = TransactionContext::new(1, run_id, 100);
    /// assert!(txn.is_active());
    /// ```
    pub fn new(txn_id: u64, run_id: RunId, start_version: u64) -> Self {
        TransactionContext {
            txn_id,
            run_id,
            start_version,
            snapshot: None,
            read_set: HashMap::new(),
            write_set: HashMap::new(),
            delete_set: HashSet::new(),
            cas_set: Vec::new(),
            status: TransactionStatus::Active,
        }
    }

    /// Create a new transaction context with a snapshot
    ///
    /// This is the primary constructor for transactions that need to read
    /// from storage. The snapshot provides a consistent point-in-time view.
    ///
    /// # Arguments
    /// * `txn_id` - Unique transaction identifier
    /// * `run_id` - Run this transaction belongs to
    /// * `snapshot` - Snapshot view for this transaction
    ///
    /// # Example
    ///
    /// ```
    /// use in_mem_concurrency::{TransactionContext, ClonedSnapshotView};
    /// use in_mem_core::types::RunId;
    /// use std::collections::BTreeMap;
    ///
    /// let run_id = RunId::new();
    /// let snapshot = Box::new(ClonedSnapshotView::empty(100));
    /// let txn = TransactionContext::with_snapshot(1, run_id, snapshot);
    /// assert!(txn.is_active());
    /// assert_eq!(txn.start_version, 100);
    /// ```
    pub fn with_snapshot(txn_id: u64, run_id: RunId, snapshot: Box<dyn SnapshotView>) -> Self {
        let start_version = snapshot.version();
        TransactionContext {
            txn_id,
            run_id,
            start_version,
            snapshot: Some(snapshot),
            read_set: HashMap::new(),
            write_set: HashMap::new(),
            delete_set: HashSet::new(),
            cas_set: Vec::new(),
            status: TransactionStatus::Active,
        }
    }

    // === Read Operations ===

    /// Get a value from the transaction
    ///
    /// Implements read-your-writes semantics:
    /// 1. Check write_set (uncommitted writes from this txn) - NO read_set entry
    /// 2. Check delete_set (uncommitted deletes from this txn) - NO read_set entry
    /// 3. Read from snapshot - tracks in read_set
    ///
    /// # Read-Set Tracking
    ///
    /// Only reads from the snapshot are tracked in read_set:
    /// - If key exists in snapshot: tracks `(key, version)`
    /// - If key doesn't exist in snapshot: tracks `(key, 0)`
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let value = txn.get(&key)?;
    /// if let Some(v) = value {
    ///     // Process value
    /// }
    /// ```
    pub fn get(&mut self, key: &Key) -> Result<Option<Value>> {
        self.ensure_active()?;

        // 1. Check write_set first (read-your-writes)
        // No read_set entry - we're reading our own uncommitted write
        if let Some(value) = self.write_set.get(key) {
            return Ok(Some(value.clone()));
        }

        // 2. Check delete_set (return None if deleted in this txn)
        // No read_set entry - we're reading our own uncommitted delete
        if self.delete_set.contains(key) {
            return Ok(None);
        }

        // 3. Read from snapshot
        self.read_from_snapshot(key)
    }

    /// Read from snapshot and track in read_set
    ///
    /// This is the core read path that tracks reads for conflict detection.
    fn read_from_snapshot(&mut self, key: &Key) -> Result<Option<Value>> {
        let snapshot = self.snapshot.as_ref().ok_or_else(|| {
            Error::InvalidState("Transaction has no snapshot for reads".to_string())
        })?;

        let versioned = snapshot.get(key)?;

        // Track in read_set for conflict detection
        if let Some(ref vv) = versioned {
            // Key exists - track its version
            self.read_set.insert(key.clone(), vv.version);
            Ok(Some(vv.value.clone()))
        } else {
            // Key doesn't exist - track with version 0
            // This is important: if someone creates this key before we commit,
            // we have a conflict (we assumed it didn't exist)
            self.read_set.insert(key.clone(), 0);
            Ok(None)
        }
    }

    /// Check if a key exists in the transaction's view
    ///
    /// This is a convenience method that calls `get()` and checks for Some.
    /// Note: This DOES track the read in read_set if the key is read from snapshot.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active.
    pub fn exists(&mut self, key: &Key) -> Result<bool> {
        Ok(self.get(key)?.is_some())
    }

    /// Scan keys with a prefix
    ///
    /// Returns all keys matching the prefix, implementing read-your-writes:
    /// - Includes uncommitted writes from this transaction matching prefix
    /// - Excludes uncommitted deletes from this transaction
    /// - Tracks all scanned keys from snapshot in read_set
    ///
    /// Results are sorted by key order.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not active or has no snapshot.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let prefix = Key::new_kv(namespace, "user:");
    /// let users = txn.scan_prefix(&prefix)?;
    /// for (key, value) in users {
    ///     // Process each user
    /// }
    /// ```
    pub fn scan_prefix(&mut self, prefix: &Key) -> Result<Vec<(Key, Value)>> {
        self.ensure_active()?;

        let snapshot = self.snapshot.as_ref().ok_or_else(|| {
            Error::InvalidState("Transaction has no snapshot for reads".to_string())
        })?;

        // Get all matching keys from snapshot
        let snapshot_results = snapshot.scan_prefix(prefix)?;

        // Build result set with read-your-writes using BTreeMap for sorted output
        let mut results: BTreeMap<Key, Value> = BTreeMap::new();

        // Add snapshot results (excluding deleted keys, tracking in read_set)
        for (key, vv) in snapshot_results {
            if !self.delete_set.contains(&key) {
                // Track in read_set
                self.read_set.insert(key.clone(), vv.version);
                results.insert(key, vv.value);
            }
            // Note: Deleted keys are NOT tracked in read_set from scan
            // because we're not "reading" them - they're excluded from results
        }

        // Add/overwrite with write_set entries matching prefix
        for (key, value) in &self.write_set {
            if key.starts_with(prefix) {
                // Write_set entries are NOT tracked in read_set
                // (they're our own uncommitted writes)
                results.insert(key.clone(), value.clone());
            }
        }

        Ok(results.into_iter().collect())
    }

    /// Get the version that was read for a key (from read_set)
    ///
    /// Returns None if the key hasn't been read from snapshot.
    /// Returns Some(0) if the key was read but didn't exist.
    pub fn get_read_version(&self, key: &Key) -> Option<u64> {
        self.read_set.get(key).copied()
    }

    // === State Management ===

    /// Check if transaction is in Active state
    ///
    /// Only active transactions can accept new read/write operations.
    pub fn is_active(&self) -> bool {
        matches!(self.status, TransactionStatus::Active)
    }

    /// Check if transaction is committed
    pub fn is_committed(&self) -> bool {
        matches!(self.status, TransactionStatus::Committed)
    }

    /// Check if transaction is aborted
    pub fn is_aborted(&self) -> bool {
        matches!(self.status, TransactionStatus::Aborted { .. })
    }

    /// Check if transaction can accept operations
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not in `Active` state.
    pub fn ensure_active(&self) -> Result<()> {
        if self.is_active() {
            Ok(())
        } else {
            Err(Error::InvalidState(format!(
                "Transaction {} is not active: {:?}",
                self.txn_id, self.status
            )))
        }
    }

    /// Transition to Validating state
    ///
    /// This is the first step of the commit process. After marking validating,
    /// the transaction should be validated against current storage state.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if not in `Active` state.
    ///
    /// # State Transition
    /// `Active` → `Validating`
    pub fn mark_validating(&mut self) -> Result<()> {
        self.ensure_active()?;
        self.status = TransactionStatus::Validating;
        Ok(())
    }

    /// Transition to Committed state
    ///
    /// Called after successful validation. The transaction's writes have been
    /// applied to storage.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if not in `Validating` state.
    ///
    /// # State Transition
    /// `Validating` → `Committed`
    pub fn mark_committed(&mut self) -> Result<()> {
        match &self.status {
            TransactionStatus::Validating => {
                self.status = TransactionStatus::Committed;
                Ok(())
            }
            _ => Err(Error::InvalidState(format!(
                "Cannot commit transaction {} from state {:?}",
                self.txn_id, self.status
            ))),
        }
    }

    /// Transition to Aborted state
    ///
    /// Can be called from `Active` (user abort) or `Validating` (conflict detected).
    /// Buffered writes are discarded - they were never applied to storage.
    ///
    /// # Arguments
    /// * `reason` - Human-readable reason for abort
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if already `Committed` or `Aborted`.
    ///
    /// # State Transitions
    /// - `Active` → `Aborted`
    /// - `Validating` → `Aborted`
    pub fn mark_aborted(&mut self, reason: String) -> Result<()> {
        match &self.status {
            TransactionStatus::Committed => Err(Error::InvalidState(format!(
                "Cannot abort committed transaction {}",
                self.txn_id
            ))),
            TransactionStatus::Aborted { .. } => Err(Error::InvalidState(format!(
                "Transaction {} already aborted",
                self.txn_id
            ))),
            _ => {
                self.status = TransactionStatus::Aborted { reason };
                Ok(())
            }
        }
    }

    // === Introspection ===

    /// Get the number of keys in the read set
    pub fn read_count(&self) -> usize {
        self.read_set.len()
    }

    /// Get the number of keys in the write set
    pub fn write_count(&self) -> usize {
        self.write_set.len()
    }

    /// Get the number of keys in the delete set
    pub fn delete_count(&self) -> usize {
        self.delete_set.len()
    }

    /// Get the number of CAS operations
    pub fn cas_count(&self) -> usize {
        self.cas_set.len()
    }

    /// Check if transaction has any pending operations
    ///
    /// Returns true if there are buffered writes, deletes, or CAS operations
    /// that would need to be applied at commit.
    pub fn has_pending_operations(&self) -> bool {
        !self.write_set.is_empty() || !self.delete_set.is_empty() || !self.cas_set.is_empty()
    }

    /// Check if transaction is read-only
    ///
    /// A read-only transaction has reads but no writes, deletes, or CAS ops.
    /// Read-only transactions always commit successfully (no conflicts possible
    /// since they don't modify anything).
    pub fn is_read_only(&self) -> bool {
        self.write_set.is_empty() && self.delete_set.is_empty() && self.cas_set.is_empty()
    }

    /// Get the abort reason if transaction is aborted
    pub fn abort_reason(&self) -> Option<&str> {
        match &self.status {
            TransactionStatus::Aborted { reason } => Some(reason),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::ClonedSnapshotView;
    use in_mem_core::types::{Namespace, TypeTag};
    use in_mem_core::value::VersionedValue;

    // === Test Helpers ===

    fn create_test_txn() -> TransactionContext {
        let run_id = RunId::new();
        TransactionContext::new(1, run_id, 100)
    }

    fn create_test_namespace() -> Namespace {
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            RunId::new(),
        )
    }

    fn create_test_key(ns: &Namespace, user_key: &[u8]) -> Key {
        Key::new(ns.clone(), TypeTag::KV, user_key.to_vec())
    }

    fn create_versioned_value(data: &[u8], version: u64) -> VersionedValue {
        VersionedValue::new(Value::Bytes(data.to_vec()), version, None)
    }

    fn create_txn_with_test_data() -> (TransactionContext, Namespace, Key, Key, Key) {
        let ns = create_test_namespace();
        let key1 = create_test_key(&ns, b"key1");
        let key2 = create_test_key(&ns, b"key2");
        let key3 = create_test_key(&ns, b"other");

        let mut data = std::collections::BTreeMap::new();
        data.insert(key1.clone(), create_versioned_value(b"value1", 10));
        data.insert(key2.clone(), create_versioned_value(b"value2", 20));
        data.insert(key3.clone(), create_versioned_value(b"other_value", 30));

        let snapshot = Box::new(ClonedSnapshotView::new(100, data));
        let run_id = RunId::new();
        let txn = TransactionContext::with_snapshot(1, run_id, snapshot);

        (txn, ns, key1, key2, key3)
    }

    // === Construction Tests ===

    #[test]
    fn test_new_transaction_is_active() {
        let txn = create_test_txn();
        assert!(txn.is_active());
        assert!(!txn.is_committed());
        assert!(!txn.is_aborted());
        assert_eq!(txn.txn_id, 1);
        assert_eq!(txn.start_version, 100);
    }

    #[test]
    fn test_new_transaction_has_empty_sets() {
        let txn = create_test_txn();
        assert_eq!(txn.read_count(), 0);
        assert_eq!(txn.write_count(), 0);
        assert_eq!(txn.delete_count(), 0);
        assert_eq!(txn.cas_count(), 0);
        assert!(!txn.has_pending_operations());
        assert!(txn.is_read_only());
    }

    #[test]
    fn test_new_transaction_preserves_run_id() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(42, run_id.clone(), 500);
        assert_eq!(txn.run_id, run_id);
        assert_eq!(txn.txn_id, 42);
        assert_eq!(txn.start_version, 500);
    }

    #[test]
    fn test_with_snapshot_sets_version() {
        let snapshot = Box::new(ClonedSnapshotView::empty(200));
        let run_id = RunId::new();
        let txn = TransactionContext::with_snapshot(1, run_id, snapshot);

        assert_eq!(txn.start_version, 200);
        assert!(txn.is_active());
    }

    // === Read Operation Tests ===

    #[test]
    fn test_get_from_snapshot() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        let result = txn.get(&key1).unwrap();
        assert!(result.is_some());

        match result.unwrap() {
            Value::Bytes(data) => assert_eq!(data, b"value1"),
            _ => panic!("Expected Bytes value"),
        }
    }

    #[test]
    fn test_get_nonexistent_key() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let nonexistent = create_test_key(&ns, b"nonexistent");

        let result = txn.get(&nonexistent).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_tracks_in_read_set() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Initially empty
        assert_eq!(txn.read_count(), 0);

        // Read a key
        let _ = txn.get(&key1).unwrap();

        // Now tracked
        assert_eq!(txn.read_count(), 1);
        assert_eq!(txn.get_read_version(&key1), Some(10)); // version from snapshot
    }

    #[test]
    fn test_get_tracks_nonexistent_with_version_zero() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let nonexistent = create_test_key(&ns, b"nonexistent");

        // Read non-existent key
        let result = txn.get(&nonexistent).unwrap();
        assert!(result.is_none());

        // Should be tracked with version 0
        assert_eq!(txn.read_count(), 1);
        assert_eq!(txn.get_read_version(&nonexistent), Some(0));
    }

    #[test]
    fn test_read_your_writes() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let new_key = create_test_key(&ns, b"new_key");

        // Write a new value
        txn.write_set
            .insert(new_key.clone(), Value::String("new_value".to_string()));

        // Read it back - should see uncommitted write
        let result = txn.get(&new_key).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), Value::String("new_value".to_string()));

        // Should NOT be in read_set (read-your-writes doesn't track)
        assert_eq!(txn.read_count(), 0);
    }

    #[test]
    fn test_read_your_writes_overwrites_snapshot() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Overwrite existing key
        txn.write_set
            .insert(key1.clone(), Value::String("overwritten".to_string()));

        // Read it back - should see uncommitted write, not snapshot
        let result = txn.get(&key1).unwrap();
        assert_eq!(result.unwrap(), Value::String("overwritten".to_string()));

        // Should NOT be in read_set
        assert_eq!(txn.read_count(), 0);
    }

    #[test]
    fn test_read_deleted_key_returns_none() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Delete a key that exists in snapshot
        txn.delete_set.insert(key1.clone());

        // Read it - should return None
        let result = txn.get(&key1).unwrap();
        assert!(result.is_none());

        // Should NOT be in read_set (read-your-deletes doesn't track)
        assert_eq!(txn.read_count(), 0);
    }

    #[test]
    fn test_read_priority_write_over_delete() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        // Both delete and write same key (edge case)
        // Write should take priority (it's checked first)
        txn.delete_set.insert(key1.clone());
        txn.write_set
            .insert(key1.clone(), Value::String("written".to_string()));

        let result = txn.get(&key1).unwrap();
        assert_eq!(result.unwrap(), Value::String("written".to_string()));
    }

    #[test]
    fn test_exists_returns_true_for_existing() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();
        assert!(txn.exists(&key1).unwrap());
    }

    #[test]
    fn test_exists_returns_false_for_nonexistent() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let nonexistent = create_test_key(&ns, b"nonexistent");
        assert!(!txn.exists(&nonexistent).unwrap());
    }

    #[test]
    fn test_exists_tracks_in_read_set() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();

        let _ = txn.exists(&key1).unwrap();

        // Should be tracked
        assert_eq!(txn.read_count(), 1);
    }

    #[test]
    fn test_get_fails_when_not_active() {
        let (mut txn, _, key1, _, _) = create_txn_with_test_data();
        txn.mark_validating().unwrap();

        let result = txn.get(&key1);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_fails_without_snapshot() {
        let mut txn = create_test_txn(); // No snapshot
        let ns = create_test_namespace();
        let key = create_test_key(&ns, b"key");

        let result = txn.get(&key);
        assert!(result.is_err());
    }

    // === Scan Prefix Tests ===

    #[test]
    fn test_scan_prefix_from_snapshot() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let prefix = create_test_key(&ns, b"key");

        let results = txn.scan_prefix(&prefix).unwrap();

        // Should find key1 and key2, not "other"
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_scan_prefix_tracks_in_read_set() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let prefix = create_test_key(&ns, b"key");

        let _ = txn.scan_prefix(&prefix).unwrap();

        // All scanned keys should be tracked
        assert_eq!(txn.read_count(), 2);
    }

    #[test]
    fn test_scan_prefix_includes_uncommitted_writes() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();

        // Add uncommitted write matching prefix
        let new_key = create_test_key(&ns, b"key_new");
        txn.write_set
            .insert(new_key.clone(), Value::String("new".to_string()));

        let prefix = create_test_key(&ns, b"key");
        let results = txn.scan_prefix(&prefix).unwrap();

        // Should include the uncommitted write
        assert_eq!(results.len(), 3); // key1, key2, key_new

        // Find the new key in results
        let found = results
            .iter()
            .any(|(k, v)| k == &new_key && *v == Value::String("new".to_string()));
        assert!(found);
    }

    #[test]
    fn test_scan_prefix_excludes_deleted_keys() {
        let (mut txn, ns, key1, _, _) = create_txn_with_test_data();

        // Delete key1
        txn.delete_set.insert(key1.clone());

        let prefix = create_test_key(&ns, b"key");
        let results = txn.scan_prefix(&prefix).unwrap();

        // Should NOT include deleted key1
        assert_eq!(results.len(), 1); // only key2

        // Verify key1 is not in results
        let found = results.iter().any(|(k, _)| k == &key1);
        assert!(!found);
    }

    #[test]
    fn test_scan_prefix_write_overwrites_snapshot() {
        let (mut txn, ns, key1, _, _) = create_txn_with_test_data();

        // Overwrite key1
        txn.write_set
            .insert(key1.clone(), Value::String("overwritten".to_string()));

        let prefix = create_test_key(&ns, b"key");
        let results = txn.scan_prefix(&prefix).unwrap();

        // Should have 2 keys
        assert_eq!(results.len(), 2);

        // key1 should have overwritten value
        let key1_value = results.iter().find(|(k, _)| k == &key1).map(|(_, v)| v);
        assert_eq!(key1_value, Some(&Value::String("overwritten".to_string())));
    }

    #[test]
    fn test_scan_prefix_empty_results() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        let prefix = create_test_key(&ns, b"nonexistent");

        let results = txn.scan_prefix(&prefix).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_scan_prefix_fails_when_not_active() {
        let (mut txn, ns, _, _, _) = create_txn_with_test_data();
        txn.mark_validating().unwrap();

        let prefix = create_test_key(&ns, b"key");
        let result = txn.scan_prefix(&prefix);
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_prefix_fails_without_snapshot() {
        let mut txn = create_test_txn();
        let ns = create_test_namespace();
        let prefix = create_test_key(&ns, b"key");

        let result = txn.scan_prefix(&prefix);
        assert!(result.is_err());
    }

    // === State Transition Tests ===

    #[test]
    fn test_state_transition_active_to_validating() {
        let mut txn = create_test_txn();
        assert!(txn.mark_validating().is_ok());
        assert!(!txn.is_active());
        assert!(matches!(txn.status, TransactionStatus::Validating));
    }

    #[test]
    fn test_state_transition_validating_to_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        assert!(txn.mark_committed().is_ok());
        assert!(txn.is_committed());
        assert!(matches!(txn.status, TransactionStatus::Committed));
    }

    #[test]
    fn test_state_transition_active_to_aborted() {
        let mut txn = create_test_txn();
        assert!(txn.mark_aborted("user requested abort".to_string()).is_ok());
        assert!(txn.is_aborted());
        assert!(matches!(txn.status, TransactionStatus::Aborted { .. }));
        assert_eq!(txn.abort_reason(), Some("user requested abort"));
    }

    #[test]
    fn test_state_transition_validating_to_aborted() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        assert!(txn.mark_aborted("conflict detected".to_string()).is_ok());
        assert!(txn.is_aborted());
        assert_eq!(txn.abort_reason(), Some("conflict detected"));
    }

    // === Invalid State Transition Tests ===

    #[test]
    fn test_cannot_validating_from_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.mark_validating();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_validating_from_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("test".to_string()).unwrap();
        let result = txn.mark_validating();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_commit_from_active() {
        let mut txn = create_test_txn();
        let result = txn.mark_committed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_commit_from_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("test".to_string()).unwrap();
        let result = txn.mark_committed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_commit_from_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.mark_committed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_abort_committed_transaction() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.mark_aborted("too late".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_abort_already_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("first abort".to_string()).unwrap();
        let result = txn.mark_aborted("second abort".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    // === ensure_active Tests ===

    #[test]
    fn test_ensure_active_succeeds_when_active() {
        let txn = create_test_txn();
        assert!(txn.ensure_active().is_ok());
    }

    #[test]
    fn test_ensure_active_fails_when_validating() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        let result = txn.ensure_active();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_ensure_active_fails_when_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.ensure_active();
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_active_fails_when_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("test".to_string()).unwrap();
        let result = txn.ensure_active();
        assert!(result.is_err());
    }

    // === Abort Reason Tests ===

    #[test]
    fn test_abort_reason_none_when_not_aborted() {
        let txn = create_test_txn();
        assert!(txn.abort_reason().is_none());
    }

    #[test]
    fn test_abort_reason_none_when_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        assert!(txn.abort_reason().is_none());
    }

    #[test]
    fn test_abort_reason_preserves_message() {
        let mut txn = create_test_txn();
        txn.mark_aborted("read-write conflict on key X".to_string())
            .unwrap();
        assert_eq!(txn.abort_reason(), Some("read-write conflict on key X"));
    }

    // === CASOperation Tests ===

    #[test]
    fn test_cas_operation_creation() {
        let run_id = RunId::new();
        let namespace = Namespace::new("t".into(), "a".into(), "g".into(), run_id);
        let key = Key::new(namespace, TypeTag::KV, b"test".to_vec());
        let value = Value::I64(42);

        let cas_op = CASOperation {
            key: key.clone(),
            expected_version: 5,
            new_value: value.clone(),
        };

        assert_eq!(cas_op.key, key);
        assert_eq!(cas_op.expected_version, 5);
        assert_eq!(cas_op.new_value, value);
    }

    #[test]
    fn test_cas_operation_version_zero_means_not_exist() {
        let run_id = RunId::new();
        let namespace = Namespace::new("t".into(), "a".into(), "g".into(), run_id);
        let key = Key::new(namespace, TypeTag::KV, b"new_key".to_vec());

        let cas_op = CASOperation {
            key,
            expected_version: 0,
            new_value: Value::String("initial".to_string()),
        };

        assert_eq!(cas_op.expected_version, 0);
    }

    // === TransactionStatus Tests ===

    #[test]
    fn test_transaction_status_equality() {
        assert_eq!(TransactionStatus::Active, TransactionStatus::Active);
        assert_eq!(TransactionStatus::Validating, TransactionStatus::Validating);
        assert_eq!(TransactionStatus::Committed, TransactionStatus::Committed);

        let aborted1 = TransactionStatus::Aborted {
            reason: "test".to_string(),
        };
        let aborted2 = TransactionStatus::Aborted {
            reason: "test".to_string(),
        };
        let aborted3 = TransactionStatus::Aborted {
            reason: "other".to_string(),
        };

        assert_eq!(aborted1, aborted2);
        assert_ne!(aborted1, aborted3);
        assert_ne!(TransactionStatus::Active, TransactionStatus::Validating);
    }

    #[test]
    fn test_transaction_status_debug() {
        let active = TransactionStatus::Active;
        let debug_str = format!("{:?}", active);
        assert!(debug_str.contains("Active"));

        let aborted = TransactionStatus::Aborted {
            reason: "conflict".to_string(),
        };
        let debug_str = format!("{:?}", aborted);
        assert!(debug_str.contains("Aborted"));
        assert!(debug_str.contains("conflict"));
    }

    #[test]
    fn test_transaction_status_clone() {
        let original = TransactionStatus::Aborted {
            reason: "test".to_string(),
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }
}
