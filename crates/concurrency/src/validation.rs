//! Transaction validation for OCC
//!
//! This module implements conflict detection per Section 3 of
//! `docs/architecture/M2_TRANSACTION_SEMANTICS.md`.
//!
//! Key rules from the spec:
//! - First-committer-wins based on READ-SET, not write-set
//! - Blind writes (write without read) do NOT conflict
//! - CAS is validated separately from read-set
//! - Write skew is ALLOWED (do not try to prevent it)

use crate::transaction::{CASOperation, TransactionContext};
use in_mem_core::traits::Storage;
use in_mem_core::types::Key;
use in_mem_core::value::Value;
use std::collections::HashMap;

/// Types of conflicts that can occur during transaction validation
///
/// See spec Section 3.1 for when each conflict type occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictType {
    /// Read-write conflict: key was read at one version but current version differs
    ///
    /// From spec Section 3.1 Condition 1:
    /// "T1 read key K and recorded version V in its read_set.
    ///  At commit time, the current storage version of K is V' where V' != V"
    ReadWriteConflict {
        /// The key that has a conflict
        key: Key,
        /// Version recorded in read_set when read
        read_version: u64,
        /// Current version in storage at validation time
        current_version: u64,
    },

    /// CAS conflict: expected version doesn't match current version
    ///
    /// From spec Section 3.1 Condition 3:
    /// "T1 called CAS(K, expected_version=V, new_value).
    ///  At commit time, current storage version of K != V"
    CASConflict {
        /// The key that has a CAS conflict
        key: Key,
        /// Expected version specified in CAS operation
        expected_version: u64,
        /// Current version in storage at validation time
        current_version: u64,
    },

    /// JSON document conflict: document version changed since read
    ///
    /// From M5 spec: Conflict occurs when a JSON document read during
    /// the transaction has been modified by another transaction.
    /// This is conservative (document-level) conflict detection.
    JsonDocConflict {
        /// The key of the JSON document with a conflict
        key: Key,
        /// Document version when read (snapshot version)
        snapshot_version: u64,
        /// Current document version at validation time
        current_version: u64,
    },

    /// JSON path read-write conflict: read and write paths overlap
    ///
    /// From M5 Epic 31: Region-based conflict detection.
    /// Conflict occurs when a read at path X overlaps with a write at path Y.
    /// Overlap means X is ancestor, descendant, or equal to Y.
    JsonPathReadWriteConflict {
        /// The key of the JSON document
        key: Key,
        /// The path that was read
        read_path: in_mem_core::json::JsonPath,
        /// The path that was written (overlaps with read_path)
        write_path: in_mem_core::json::JsonPath,
    },

    /// JSON path write-write conflict: two writes to overlapping paths
    ///
    /// From M5 Epic 31: Region-based conflict detection.
    /// Conflict occurs when two writes within the same transaction target
    /// overlapping paths. This is a semantic error.
    JsonPathWriteWriteConflict {
        /// The key of the JSON document
        key: Key,
        /// The first write path
        path1: in_mem_core::json::JsonPath,
        /// The second write path (overlaps with path1)
        path2: in_mem_core::json::JsonPath,
    },
}

/// Result of transaction validation
///
/// Accumulates all conflicts found during validation.
/// A transaction commits only if is_valid() returns true.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// All conflicts detected during validation
    pub conflicts: Vec<ConflictType>,
}

impl ValidationResult {
    /// Create a successful validation result (no conflicts)
    pub fn ok() -> Self {
        ValidationResult {
            conflicts: Vec::new(),
        }
    }

    /// Create a validation result with a single conflict
    pub fn conflict(conflict: ConflictType) -> Self {
        ValidationResult {
            conflicts: vec![conflict],
        }
    }

    /// Check if validation passed (no conflicts)
    pub fn is_valid(&self) -> bool {
        self.conflicts.is_empty()
    }

    /// Merge another validation result into this one
    ///
    /// Used to combine results from different validation phases.
    pub fn merge(&mut self, other: ValidationResult) {
        self.conflicts.extend(other.conflicts);
    }

    /// Get the number of conflicts
    pub fn conflict_count(&self) -> usize {
        self.conflicts.len()
    }
}

/// Validate the read-set against current storage state
///
/// Per spec Section 3.1 Condition 1:
/// - For each key in read_set, check if current version matches read version
/// - If any version changed, report ReadWriteConflict
///
/// # Arguments
/// * `read_set` - Keys read with their versions at read time
/// * `store` - Storage to check current versions against
///
/// # Returns
/// ValidationResult with any ReadWriteConflicts found
pub fn validate_read_set<S: Storage>(read_set: &HashMap<Key, u64>, store: &S) -> ValidationResult {
    let mut result = ValidationResult::ok();

    for (key, read_version) in read_set {
        // Get current version from storage
        let current_version = match store.get(key) {
            Ok(Some(vv)) => vv.version,
            Ok(None) => 0, // Key doesn't exist = version 0
            Err(_) => {
                // Storage error - treat as version 0 (conservative)
                0
            }
        };

        // Check if version changed
        if current_version != *read_version {
            result.conflicts.push(ConflictType::ReadWriteConflict {
                key: key.clone(),
                read_version: *read_version,
                current_version,
            });
        }
    }

    result
}

/// Validate the write-set against current storage state
///
/// Per spec Section 3.2 Scenario 1 (Blind Write):
/// - Blind writes (write without read) do NOT conflict
/// - First-committer-wins is based on READ-SET, not write-set
///
/// This function always returns OK because:
/// - If key was read → conflict detected by validate_read_set()
/// - If key was NOT read (blind write) → no conflict
///
/// # Arguments
/// * `write_set` - Keys to be written with their new values
/// * `_read_set` - Keys that were read (for context, not used)
/// * `_start_version` - Transaction's start version (not used)
/// * `_store` - Storage to check (not used)
///
/// # Returns
/// ValidationResult (always valid for pure blind writes)
#[allow(clippy::ptr_arg)]
pub fn validate_write_set<S: Storage>(
    write_set: &HashMap<Key, Value>,
    _read_set: &HashMap<Key, u64>,
    _start_version: u64,
    _store: &S,
) -> ValidationResult {
    // Per spec: Blind writes do NOT conflict
    // Write-write conflict is only detected when the key was ALSO READ
    // That case is handled by validate_read_set()
    //
    // From spec Section 3.2:
    // "First-committer-wins is based on the READ-SET, not the write-set."

    // Note: We could add optional write-write conflict detection here
    // for keys in BOTH write_set AND read_set, but that's redundant
    // with read-set validation. Keeping this simple per spec.

    let _ = write_set; // Acknowledge parameter (used for type checking)

    ValidationResult::ok()
}

/// Validate CAS operations against current storage state
///
/// Per spec Section 3.1 Condition 3:
/// - For each CAS op, check if current version matches expected_version
/// - If versions don't match, report CASConflict
///
/// Per spec Section 3.4:
/// - CAS does NOT add to read_set (validated separately)
/// - expected_version=0 means "key must not exist"
///
/// # Arguments
/// * `cas_set` - CAS operations to validate
/// * `store` - Storage to check current versions against
///
/// # Returns
/// ValidationResult with any CASConflicts found
pub fn validate_cas_set<S: Storage>(cas_set: &[CASOperation], store: &S) -> ValidationResult {
    let mut result = ValidationResult::ok();

    for cas_op in cas_set {
        // Get current version from storage
        let current_version = match store.get(&cas_op.key) {
            Ok(Some(vv)) => vv.version,
            Ok(None) => 0, // Key doesn't exist = version 0
            Err(_) => 0,   // Storage error = treat as non-existent
        };

        // Check if expected version matches
        if current_version != cas_op.expected_version {
            result.conflicts.push(ConflictType::CASConflict {
                key: cas_op.key.clone(),
                expected_version: cas_op.expected_version,
                current_version,
            });
        }
    }

    result
}

/// Validate JSON document versions against current storage state
///
/// Per M5 spec: JSON conflict detection is document-level (conservative).
/// If any JSON document read during the transaction has been modified,
/// the transaction must abort.
///
/// # Arguments
/// * `json_snapshot_versions` - Document keys and their versions at read time
/// * `store` - Storage to check current versions against
///
/// # Returns
/// ValidationResult with any JsonDocConflicts found
pub fn validate_json_set<S: Storage>(
    json_snapshot_versions: Option<&HashMap<Key, u64>>,
    store: &S,
) -> ValidationResult {
    let mut result = ValidationResult::ok();

    let Some(versions) = json_snapshot_versions else {
        return result; // No JSON operations = no JSON conflicts
    };

    for (key, snapshot_version) in versions {
        // Get current version from storage
        let current_version = match store.get(key) {
            Ok(Some(vv)) => vv.version,
            Ok(None) => 0, // Document deleted = version 0
            Err(_) => 0,   // Storage error = treat as deleted
        };

        // Check if version changed since transaction read it
        if current_version != *snapshot_version {
            result.conflicts.push(ConflictType::JsonDocConflict {
                key: key.clone(),
                snapshot_version: *snapshot_version,
                current_version,
            });
        }
    }

    result
}

/// Validate JSON path-level conflicts (M5 Epic 31)
///
/// This provides region-based conflict detection for JSON operations.
/// It checks for:
/// - Write-write conflicts: Two writes to overlapping paths within the transaction
///
/// Note: Read-write path conflicts are intentionally NOT checked here because
/// reading a path and then writing to an overlapping path is valid behavior
/// (read-your-writes semantics). The version-based conflict detection in
/// `validate_json_set` already handles the case where concurrent transactions
/// modify the same document.
///
/// # Arguments
/// * `json_reads` - JSON paths that were read during the transaction
/// * `json_writes` - JSON patches to be applied
///
/// # Returns
/// ValidationResult with any path conflicts found
pub fn validate_json_paths(
    json_reads: &[crate::transaction::JsonPathRead],
    json_writes: &[crate::transaction::JsonPatchEntry],
) -> ValidationResult {
    use crate::conflict::{check_write_write_conflicts, ConflictResult};

    let mut result = ValidationResult::ok();

    // Check for write-write conflicts (overlapping write paths)
    // This is a semantic error - the order of writes matters and the result is undefined
    for conflict in check_write_write_conflicts(json_writes) {
        if let ConflictResult::WriteWriteConflict { key, path1, path2 } = conflict {
            result
                .conflicts
                .push(ConflictType::JsonPathWriteWriteConflict { key, path1, path2 });
        }
    }

    // Note: We intentionally do NOT check read-write path conflicts here.
    // Reading a path and then writing to it (or a parent/child path) is valid
    // behavior within a single transaction. The document-level version check
    // handles concurrent modification by other transactions.
    let _ = json_reads; // Acknowledge the parameter

    result
}

/// Validate a complete transaction against current storage state
///
/// Per spec Section 3 (Conflict Detection):
/// 1. Validates read-set: detects read-write conflicts (first-committer-wins)
/// 2. Validates write-set: currently no-op (blind writes don't conflict)
/// 3. Validates CAS-set: ensures expected versions still match
/// 4. Validates JSON-set: ensures JSON document versions haven't changed (M5)
/// 5. Validates JSON paths: ensures no overlapping writes within transaction (M5 Epic 31)
///
/// **Per spec Section 3.2 Scenario 3**: Read-only transactions ALWAYS succeed.
/// If a transaction has no writes (empty write_set, delete_set, cas_set, and json_writes),
/// validation is skipped entirely and the transaction succeeds.
///
/// # Arguments
/// * `txn` - Transaction to validate (should be in Validating state for correctness,
///   but this function doesn't enforce that)
/// * `store` - Current storage state to validate against
///
/// # Returns
/// ValidationResult containing any conflicts found
///
/// # Spec Reference
/// - Section 3.1: When conflicts occur
/// - Section 3.2: Conflict scenarios (including read-only transaction rule)
/// - Section 3.3: First-committer-wins rule
/// - M5: JSON document-level conflict detection
pub fn validate_transaction<S: Storage>(txn: &TransactionContext, store: &S) -> ValidationResult {
    // Per spec Section 3.2 Scenario 3: Read-only transactions ALWAYS commit.
    // "Read-Only Transaction: T1 only reads keys, never writes any → ALWAYS COMMITS"
    // "Why: Read-only transactions have no writes to validate. They simply return their snapshot view."
    // Note: We also need to check for JSON writes
    if txn.is_read_only() && txn.json_writes().is_empty() {
        return ValidationResult::ok();
    }

    let mut result = ValidationResult::ok();

    // 1. Validate read-set (detects read-write conflicts)
    result.merge(validate_read_set(&txn.read_set, store));

    // 2. Validate write-set (currently no-op, but may detect conflicts in future)
    result.merge(validate_write_set(
        &txn.write_set,
        &txn.read_set,
        txn.start_version,
        store,
    ));

    // 3. Validate CAS-set (detects version mismatches)
    result.merge(validate_cas_set(&txn.cas_set, store));

    // 4. Validate JSON-set (detects JSON document version changes)
    result.merge(validate_json_set(txn.json_snapshot_versions(), store));

    // 5. Validate JSON paths (detects overlapping writes within transaction)
    result.merge(validate_json_paths(txn.json_reads(), txn.json_writes()));

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use in_mem_core::types::{Namespace, RunId, TypeTag};

    fn create_test_key(name: &[u8]) -> Key {
        let ns = Namespace::new("test".into(), "app".into(), "agent".into(), RunId::new());
        Key::new(ns, TypeTag::KV, name.to_vec())
    }

    // === ValidationResult Tests ===

    #[test]
    fn test_validation_result_ok() {
        let result = ValidationResult::ok();
        assert!(result.is_valid());
        assert_eq!(result.conflict_count(), 0);
    }

    #[test]
    fn test_validation_result_conflict() {
        let key = create_test_key(b"test_key");
        let conflict = ConflictType::ReadWriteConflict {
            key: key.clone(),
            read_version: 10,
            current_version: 20,
        };
        let result = ValidationResult::conflict(conflict);

        assert!(!result.is_valid());
        assert_eq!(result.conflict_count(), 1);
    }

    #[test]
    fn test_validation_result_merge() {
        let key1 = create_test_key(b"key1");
        let key2 = create_test_key(b"key2");

        let mut result1 = ValidationResult::conflict(ConflictType::ReadWriteConflict {
            key: key1,
            read_version: 10,
            current_version: 20,
        });
        let result2 = ValidationResult::conflict(ConflictType::CASConflict {
            key: key2,
            expected_version: 5,
            current_version: 10,
        });

        result1.merge(result2);

        assert_eq!(result1.conflict_count(), 2);
        assert!(!result1.is_valid());
    }

    #[test]
    fn test_validation_result_merge_ok_with_ok() {
        let mut result1 = ValidationResult::ok();
        let result2 = ValidationResult::ok();

        result1.merge(result2);

        assert!(result1.is_valid());
        assert_eq!(result1.conflict_count(), 0);
    }

    #[test]
    fn test_validation_result_merge_ok_with_conflict() {
        let key = create_test_key(b"key");
        let mut result1 = ValidationResult::ok();
        let result2 = ValidationResult::conflict(ConflictType::CASConflict {
            key,
            expected_version: 0,
            current_version: 5,
        });

        result1.merge(result2);

        assert!(!result1.is_valid());
        assert_eq!(result1.conflict_count(), 1);
    }

    // === ConflictType Tests ===

    #[test]
    fn test_read_write_conflict_creation() {
        let key = create_test_key(b"test");
        let conflict = ConflictType::ReadWriteConflict {
            key: key.clone(),
            read_version: 100,
            current_version: 105,
        };

        match conflict {
            ConflictType::ReadWriteConflict {
                key: k,
                read_version,
                current_version,
            } => {
                assert_eq!(k, key);
                assert_eq!(read_version, 100);
                assert_eq!(current_version, 105);
            }
            _ => panic!("Wrong conflict type"),
        }
    }

    #[test]
    fn test_cas_conflict_creation() {
        let key = create_test_key(b"counter");
        let conflict = ConflictType::CASConflict {
            key: key.clone(),
            expected_version: 0,
            current_version: 1,
        };

        match conflict {
            ConflictType::CASConflict {
                key: k,
                expected_version,
                current_version,
            } => {
                assert_eq!(k, key);
                assert_eq!(expected_version, 0);
                assert_eq!(current_version, 1);
            }
            _ => panic!("Wrong conflict type"),
        }
    }

    #[test]
    fn test_conflict_type_equality() {
        let key = create_test_key(b"key1");

        let conflict1 = ConflictType::ReadWriteConflict {
            key: key.clone(),
            read_version: 10,
            current_version: 20,
        };
        let conflict2 = ConflictType::ReadWriteConflict {
            key: key.clone(),
            read_version: 10,
            current_version: 20,
        };

        assert_eq!(conflict1, conflict2);
    }

    #[test]
    fn test_conflict_type_debug() {
        let key = create_test_key(b"key");
        let conflict = ConflictType::CASConflict {
            key,
            expected_version: 5,
            current_version: 10,
        };

        let debug_str = format!("{:?}", conflict);
        assert!(debug_str.contains("CASConflict"));
        assert!(debug_str.contains("expected_version: 5"));
    }

    // === Read-Set Validation Tests ===

    mod read_set_tests {
        use super::*;
        use in_mem_core::value::Value;
        use in_mem_storage::UnifiedStore;

        fn create_test_store() -> UnifiedStore {
            UnifiedStore::new()
        }

        fn create_test_namespace() -> Namespace {
            Namespace::new("t".into(), "a".into(), "g".into(), RunId::new())
        }

        fn create_key(ns: &Namespace, name: &[u8]) -> Key {
            Key::new(ns.clone(), TypeTag::KV, name.to_vec())
        }

        #[test]
        fn test_validate_read_set_empty() {
            let store = create_test_store();
            let read_set: HashMap<Key, u64> = HashMap::new();

            let result = validate_read_set(&read_set, &store);

            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_read_set_version_unchanged() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"key1");

            // Put key at version
            store
                .put(key.clone(), Value::Bytes(b"value".to_vec()), None)
                .unwrap();
            let current_version = store.get(&key).unwrap().unwrap().version;

            // Read-set records the same version
            let mut read_set = HashMap::new();
            read_set.insert(key.clone(), current_version);

            let result = validate_read_set(&read_set, &store);

            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_read_set_version_changed() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"key1");

            // Put key at version 1
            store
                .put(key.clone(), Value::Bytes(b"v1".to_vec()), None)
                .unwrap();
            let v1 = store.get(&key).unwrap().unwrap().version;

            // Another transaction modified it (version 2)
            store
                .put(key.clone(), Value::Bytes(b"v2".to_vec()), None)
                .unwrap();

            // Read-set still has old version
            let mut read_set = HashMap::new();
            read_set.insert(key.clone(), v1);

            let result = validate_read_set(&read_set, &store);

            assert!(!result.is_valid());
            assert_eq!(result.conflict_count(), 1);
            match &result.conflicts[0] {
                ConflictType::ReadWriteConflict {
                    key: k,
                    read_version,
                    current_version,
                } => {
                    assert_eq!(k, &key);
                    assert_eq!(*read_version, v1);
                    assert!(*current_version > v1);
                }
                _ => panic!("Expected ReadWriteConflict"),
            }
        }

        #[test]
        fn test_validate_read_set_key_deleted() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"key1");

            // Put then delete
            store
                .put(key.clone(), Value::Bytes(b"value".to_vec()), None)
                .unwrap();
            let version_when_read = store.get(&key).unwrap().unwrap().version;
            store.delete(&key).unwrap();

            // Read-set has version from when key existed
            let mut read_set = HashMap::new();
            read_set.insert(key.clone(), version_when_read);

            let result = validate_read_set(&read_set, &store);

            assert!(!result.is_valid());
            match &result.conflicts[0] {
                ConflictType::ReadWriteConflict {
                    current_version, ..
                } => {
                    // Deleted key has version 0 (doesn't exist)
                    assert_eq!(*current_version, 0);
                }
                _ => panic!("Expected ReadWriteConflict"),
            }
        }

        #[test]
        fn test_validate_read_set_key_created_after_read() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"key1");

            // Read-set recorded key as non-existent (version 0)
            let mut read_set = HashMap::new();
            read_set.insert(key.clone(), 0);

            // Another transaction created the key
            store
                .put(key.clone(), Value::Bytes(b"value".to_vec()), None)
                .unwrap();

            let result = validate_read_set(&read_set, &store);

            assert!(!result.is_valid());
            match &result.conflicts[0] {
                ConflictType::ReadWriteConflict {
                    read_version,
                    current_version,
                    ..
                } => {
                    assert_eq!(*read_version, 0);
                    assert!(*current_version > 0);
                }
                _ => panic!("Expected ReadWriteConflict"),
            }
        }

        #[test]
        fn test_validate_read_set_multiple_conflicts() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_key(&ns, b"key1");
            let key2 = create_key(&ns, b"key2");

            // Put both keys
            store
                .put(key1.clone(), Value::Bytes(b"v1".to_vec()), None)
                .unwrap();
            store
                .put(key2.clone(), Value::Bytes(b"v1".to_vec()), None)
                .unwrap();
            let v1_1 = store.get(&key1).unwrap().unwrap().version;
            let v1_2 = store.get(&key2).unwrap().unwrap().version;

            // Both keys modified
            store
                .put(key1.clone(), Value::Bytes(b"v2".to_vec()), None)
                .unwrap();
            store
                .put(key2.clone(), Value::Bytes(b"v2".to_vec()), None)
                .unwrap();

            // Read-set has old versions
            let mut read_set = HashMap::new();
            read_set.insert(key1.clone(), v1_1);
            read_set.insert(key2.clone(), v1_2);

            let result = validate_read_set(&read_set, &store);

            assert!(!result.is_valid());
            assert_eq!(result.conflict_count(), 2);
        }

        #[test]
        fn test_validate_read_set_partial_conflict() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_key(&ns, b"key1");
            let key2 = create_key(&ns, b"key2");

            // Put both keys
            store
                .put(key1.clone(), Value::Bytes(b"v1".to_vec()), None)
                .unwrap();
            store
                .put(key2.clone(), Value::Bytes(b"v1".to_vec()), None)
                .unwrap();
            let v1_1 = store.get(&key1).unwrap().unwrap().version;
            let v1_2 = store.get(&key2).unwrap().unwrap().version;

            // Only key1 modified
            store
                .put(key1.clone(), Value::Bytes(b"v2".to_vec()), None)
                .unwrap();

            // Read-set has old versions for both
            let mut read_set = HashMap::new();
            read_set.insert(key1.clone(), v1_1);
            read_set.insert(key2.clone(), v1_2);

            let result = validate_read_set(&read_set, &store);

            // Only one conflict (key1)
            assert!(!result.is_valid());
            assert_eq!(result.conflict_count(), 1);
        }

        #[test]
        fn test_validate_read_set_nonexistent_stays_nonexistent() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"key1");

            // Key never existed, read-set recorded version 0
            let mut read_set = HashMap::new();
            read_set.insert(key.clone(), 0);

            // Key still doesn't exist - no conflict
            let result = validate_read_set(&read_set, &store);

            assert!(result.is_valid());
        }
    }

    // === Write-Set Validation Tests ===

    mod write_set_tests {
        use super::*;
        use in_mem_core::value::Value;
        use in_mem_storage::UnifiedStore;

        fn create_test_store() -> UnifiedStore {
            UnifiedStore::new()
        }

        fn create_test_namespace() -> Namespace {
            Namespace::new("t".into(), "a".into(), "g".into(), RunId::new())
        }

        fn create_key(ns: &Namespace, name: &[u8]) -> Key {
            Key::new(ns.clone(), TypeTag::KV, name.to_vec())
        }

        #[test]
        fn test_validate_write_set_empty() {
            let store = create_test_store();
            let write_set: HashMap<Key, Value> = HashMap::new();
            let read_set: HashMap<Key, u64> = HashMap::new();

            let result = validate_write_set(&write_set, &read_set, 100, &store);

            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_write_set_blind_write_no_conflict() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"key1");

            // Put initial value
            store
                .put(key.clone(), Value::Bytes(b"initial".to_vec()), None)
                .unwrap();
            let start_version = store.current_version();

            // Another transaction modified the key
            store
                .put(key.clone(), Value::Bytes(b"concurrent".to_vec()), None)
                .unwrap();

            // Our write_set has the key (blind write - not in read_set)
            let mut write_set = HashMap::new();
            write_set.insert(key.clone(), Value::Bytes(b"our_write".to_vec()));
            let read_set: HashMap<Key, u64> = HashMap::new(); // Empty - blind write

            // Per spec: Blind writes do NOT conflict
            let result = validate_write_set(&write_set, &read_set, start_version, &store);

            assert!(result.is_valid(), "Blind writes should not conflict");
        }

        #[test]
        fn test_validate_write_set_multiple_blind_writes() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_key(&ns, b"key1");
            let key2 = create_key(&ns, b"key2");

            // Put initial values
            store
                .put(key1.clone(), Value::Bytes(b"v1".to_vec()), None)
                .unwrap();
            store
                .put(key2.clone(), Value::Bytes(b"v1".to_vec()), None)
                .unwrap();
            let start_version = store.current_version();

            // Both modified by concurrent transaction
            store
                .put(key1.clone(), Value::Bytes(b"v2".to_vec()), None)
                .unwrap();
            store
                .put(key2.clone(), Value::Bytes(b"v2".to_vec()), None)
                .unwrap();

            // Blind writes to both
            let mut write_set = HashMap::new();
            write_set.insert(key1, Value::Bytes(b"our1".to_vec()));
            write_set.insert(key2, Value::Bytes(b"our2".to_vec()));
            let read_set: HashMap<Key, u64> = HashMap::new();

            let result = validate_write_set(&write_set, &read_set, start_version, &store);

            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_write_set_to_new_key() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"new_key");

            let mut write_set = HashMap::new();
            write_set.insert(key, Value::Bytes(b"new_value".to_vec()));
            let read_set: HashMap<Key, u64> = HashMap::new();

            let result = validate_write_set(&write_set, &read_set, 100, &store);

            assert!(result.is_valid());
        }

        /// This test documents that write-set validation alone doesn't detect conflicts.
        /// The read-set validation is what catches write-write conflicts on read keys.
        #[test]
        fn test_write_set_validation_does_not_detect_read_key_conflicts() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"key1");

            store
                .put(key.clone(), Value::Bytes(b"initial".to_vec()), None)
                .unwrap();
            let read_version = store.get(&key).unwrap().unwrap().version;
            let start_version = store.current_version();

            // Key modified by concurrent transaction
            store
                .put(key.clone(), Value::Bytes(b"concurrent".to_vec()), None)
                .unwrap();

            // Key in BOTH read_set AND write_set
            let mut write_set = HashMap::new();
            write_set.insert(key.clone(), Value::Bytes(b"our_write".to_vec()));
            let mut read_set = HashMap::new();
            read_set.insert(key.clone(), read_version);

            // Write-set validation still returns OK
            let write_result = validate_write_set(&write_set, &read_set, start_version, &store);
            assert!(write_result.is_valid());

            // But read-set validation catches the conflict
            let read_result = validate_read_set(&read_set, &store);
            assert!(!read_result.is_valid());
        }
    }

    // === CAS Validation Tests ===

    mod cas_tests {
        use super::*;
        use crate::CASOperation;
        use in_mem_core::value::Value;
        use in_mem_storage::UnifiedStore;

        fn create_test_store() -> UnifiedStore {
            UnifiedStore::new()
        }

        fn create_test_namespace() -> Namespace {
            Namespace::new("t".into(), "a".into(), "g".into(), RunId::new())
        }

        fn create_key(ns: &Namespace, name: &[u8]) -> Key {
            Key::new(ns.clone(), TypeTag::KV, name.to_vec())
        }

        #[test]
        fn test_validate_cas_set_empty() {
            let store = create_test_store();
            let cas_set: Vec<CASOperation> = Vec::new();

            let result = validate_cas_set(&cas_set, &store);

            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_cas_version_matches() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"counter");

            // Put key
            store.put(key.clone(), Value::I64(100), None).unwrap();
            let current_version = store.get(&key).unwrap().unwrap().version;

            // CAS with matching version
            let cas_set = vec![CASOperation {
                key: key.clone(),
                expected_version: current_version,
                new_value: Value::I64(101),
            }];

            let result = validate_cas_set(&cas_set, &store);

            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_cas_version_mismatch() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"counter");

            // Put key
            store.put(key.clone(), Value::I64(100), None).unwrap();
            let v1 = store.get(&key).unwrap().unwrap().version;

            // Concurrent transaction modifies it
            store.put(key.clone(), Value::I64(200), None).unwrap();

            // CAS with old version
            let cas_set = vec![CASOperation {
                key: key.clone(),
                expected_version: v1,
                new_value: Value::I64(101),
            }];

            let result = validate_cas_set(&cas_set, &store);

            assert!(!result.is_valid());
            assert_eq!(result.conflict_count(), 1);
            match &result.conflicts[0] {
                ConflictType::CASConflict {
                    expected_version,
                    current_version,
                    ..
                } => {
                    assert_eq!(*expected_version, v1);
                    assert!(*current_version > v1);
                }
                _ => panic!("Expected CASConflict"),
            }
        }

        #[test]
        fn test_validate_cas_version_zero_key_not_exists() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"new_key");

            // CAS with expected_version=0 on non-existent key (should succeed)
            let cas_set = vec![CASOperation {
                key: key.clone(),
                expected_version: 0,
                new_value: Value::String("initial".into()),
            }];

            let result = validate_cas_set(&cas_set, &store);

            assert!(
                result.is_valid(),
                "CAS with version 0 on non-existent key should succeed"
            );
        }

        #[test]
        fn test_validate_cas_version_zero_key_exists() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"existing_key");

            // Key exists
            store
                .put(key.clone(), Value::String("exists".into()), None)
                .unwrap();

            // CAS with expected_version=0 should fail (key exists)
            let cas_set = vec![CASOperation {
                key: key.clone(),
                expected_version: 0,
                new_value: Value::String("new".into()),
            }];

            let result = validate_cas_set(&cas_set, &store);

            assert!(!result.is_valid());
            match &result.conflicts[0] {
                ConflictType::CASConflict {
                    expected_version,
                    current_version,
                    ..
                } => {
                    assert_eq!(*expected_version, 0);
                    assert!(*current_version > 0);
                }
                _ => panic!("Expected CASConflict"),
            }
        }

        #[test]
        fn test_validate_cas_nonzero_version_key_not_exists() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key = create_key(&ns, b"missing_key");

            // CAS expecting version 5 on non-existent key (should fail)
            let cas_set = vec![CASOperation {
                key: key.clone(),
                expected_version: 5,
                new_value: Value::I64(10),
            }];

            let result = validate_cas_set(&cas_set, &store);

            assert!(!result.is_valid());
            match &result.conflicts[0] {
                ConflictType::CASConflict {
                    expected_version,
                    current_version,
                    ..
                } => {
                    assert_eq!(*expected_version, 5);
                    assert_eq!(*current_version, 0); // Key doesn't exist
                }
                _ => panic!("Expected CASConflict"),
            }
        }

        #[test]
        fn test_validate_cas_multiple_operations() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_key(&ns, b"key1");
            let key2 = create_key(&ns, b"key2");

            store.put(key1.clone(), Value::I64(1), None).unwrap();
            store.put(key2.clone(), Value::I64(2), None).unwrap();
            let v1 = store.get(&key1).unwrap().unwrap().version;
            let v2 = store.get(&key2).unwrap().unwrap().version;

            let cas_set = vec![
                CASOperation {
                    key: key1.clone(),
                    expected_version: v1,
                    new_value: Value::I64(10),
                },
                CASOperation {
                    key: key2.clone(),
                    expected_version: v2,
                    new_value: Value::I64(20),
                },
            ];

            let result = validate_cas_set(&cas_set, &store);

            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_cas_multiple_partial_conflict() {
            let store = create_test_store();
            let ns = create_test_namespace();
            let key1 = create_key(&ns, b"key1");
            let key2 = create_key(&ns, b"key2");

            store.put(key1.clone(), Value::I64(1), None).unwrap();
            store.put(key2.clone(), Value::I64(2), None).unwrap();
            let v1 = store.get(&key1).unwrap().unwrap().version;
            let v2 = store.get(&key2).unwrap().unwrap().version;

            // Modify only key1
            store.put(key1.clone(), Value::I64(10), None).unwrap();

            let cas_set = vec![
                CASOperation {
                    key: key1.clone(),
                    expected_version: v1, // Old version - will conflict
                    new_value: Value::I64(100),
                },
                CASOperation {
                    key: key2.clone(),
                    expected_version: v2, // Current version - OK
                    new_value: Value::I64(200),
                },
            ];

            let result = validate_cas_set(&cas_set, &store);

            assert!(!result.is_valid());
            assert_eq!(result.conflict_count(), 1); // Only key1 conflicts
        }
    }

    // === JSON Validation Tests (M5 Story #285) ===
    mod json_validation_tests {
        use super::*;
        use in_mem_core::types::TypeTag;
        use in_mem_core::JsonDocId;
        use in_mem_storage::UnifiedStore;

        fn create_json_test_store() -> UnifiedStore {
            UnifiedStore::new()
        }

        fn create_json_test_namespace() -> Namespace {
            Namespace::new("test".into(), "app".into(), "agent".into(), RunId::new())
        }

        fn create_json_key(ns: &Namespace, doc_id: &JsonDocId) -> Key {
            Key::new(ns.clone(), TypeTag::Json, doc_id.as_bytes().to_vec())
        }

        #[test]
        fn test_validate_json_set_none() {
            let store = create_json_test_store();
            let result = validate_json_set(None, &store);
            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_json_set_empty() {
            let store = create_json_test_store();
            let versions = HashMap::new();
            let result = validate_json_set(Some(&versions), &store);
            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_json_set_version_match() {
            let store = create_json_test_store();
            let ns = create_json_test_namespace();
            let doc_id = JsonDocId::new();
            let key = create_json_key(&ns, &doc_id);

            // Add document to store
            store
                .put(key.clone(), Value::Bytes(b"{}".to_vec()), None)
                .unwrap();
            let version = store.get(&key).unwrap().unwrap().version;

            // Create snapshot versions matching current state
            let mut versions = HashMap::new();
            versions.insert(key, version);

            let result = validate_json_set(Some(&versions), &store);
            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_json_set_version_mismatch() {
            let store = create_json_test_store();
            let ns = create_json_test_namespace();
            let doc_id = JsonDocId::new();
            let key = create_json_key(&ns, &doc_id);

            // Add document to store
            store
                .put(key.clone(), Value::Bytes(b"{}".to_vec()), None)
                .unwrap();
            let old_version = store.get(&key).unwrap().unwrap().version;

            // Modify the document
            store
                .put(
                    key.clone(),
                    Value::Bytes(b"{\"updated\":true}".to_vec()),
                    None,
                )
                .unwrap();

            // Use old version in snapshot
            let mut versions = HashMap::new();
            versions.insert(key, old_version);

            let result = validate_json_set(Some(&versions), &store);
            assert!(!result.is_valid());
            assert_eq!(result.conflict_count(), 1);

            match &result.conflicts[0] {
                ConflictType::JsonDocConflict {
                    snapshot_version, ..
                } => {
                    assert_eq!(*snapshot_version, old_version);
                }
                _ => panic!("Expected JsonDocConflict"),
            }
        }

        #[test]
        fn test_validate_json_set_document_deleted() {
            let store = create_json_test_store();
            let ns = create_json_test_namespace();
            let doc_id = JsonDocId::new();
            let key = create_json_key(&ns, &doc_id);

            // Add document to store
            store
                .put(key.clone(), Value::Bytes(b"{}".to_vec()), None)
                .unwrap();
            let version = store.get(&key).unwrap().unwrap().version;

            // Delete the document
            store.delete(&key).unwrap();

            // Use old version in snapshot
            let mut versions = HashMap::new();
            versions.insert(key, version);

            let result = validate_json_set(Some(&versions), &store);
            assert!(!result.is_valid());
            assert_eq!(result.conflict_count(), 1);

            // Deleted document should show current_version = 0
            match &result.conflicts[0] {
                ConflictType::JsonDocConflict {
                    current_version, ..
                } => {
                    assert_eq!(*current_version, 0);
                }
                _ => panic!("Expected JsonDocConflict"),
            }
        }

        #[test]
        fn test_json_doc_conflict_creation() {
            let key = create_test_key(b"doc");
            let conflict = ConflictType::JsonDocConflict {
                key: key.clone(),
                snapshot_version: 10,
                current_version: 15,
            };

            match conflict {
                ConflictType::JsonDocConflict {
                    key: k,
                    snapshot_version,
                    current_version,
                } => {
                    assert_eq!(k, key);
                    assert_eq!(snapshot_version, 10);
                    assert_eq!(current_version, 15);
                }
                _ => panic!("Wrong conflict type"),
            }
        }
    }

    mod json_path_validation_tests {
        use super::*;
        use crate::transaction::{JsonPatchEntry, JsonPathRead};
        use in_mem_core::json::{JsonPatch, JsonPath};
        use in_mem_core::types::{JsonDocId, Namespace, RunId};

        fn create_json_key() -> Key {
            let ns = Namespace::for_run(RunId::new());
            Key::new_json(ns, &JsonDocId::new())
        }

        #[test]
        fn test_validate_json_paths_no_writes() {
            let reads = vec![];
            let writes = vec![];

            let result = validate_json_paths(&reads, &writes);
            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_json_paths_disjoint_writes() {
            let key = create_json_key();

            let reads = vec![];
            let writes = vec![
                JsonPatchEntry::new(
                    key.clone(),
                    JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(1).into()),
                    2,
                ),
                JsonPatchEntry::new(
                    key.clone(),
                    JsonPatch::set_at("bar".parse().unwrap(), serde_json::json!(2).into()),
                    3,
                ),
            ];

            let result = validate_json_paths(&reads, &writes);
            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_json_paths_overlapping_writes() {
            let key = create_json_key();

            let reads = vec![];
            let writes = vec![
                JsonPatchEntry::new(
                    key.clone(),
                    JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(1).into()),
                    2,
                ),
                JsonPatchEntry::new(
                    key.clone(),
                    JsonPatch::set_at("foo.bar".parse().unwrap(), serde_json::json!(2).into()),
                    3,
                ),
            ];

            let result = validate_json_paths(&reads, &writes);
            assert!(!result.is_valid());
            assert_eq!(result.conflict_count(), 1);
            assert!(matches!(
                result.conflicts[0],
                ConflictType::JsonPathWriteWriteConflict { .. }
            ));
        }

        #[test]
        fn test_validate_json_paths_exact_same_path() {
            let key = create_json_key();

            let reads = vec![];
            let writes = vec![
                JsonPatchEntry::new(
                    key.clone(),
                    JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(1).into()),
                    2,
                ),
                JsonPatchEntry::new(
                    key.clone(),
                    JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(2).into()),
                    3,
                ),
            ];

            let result = validate_json_paths(&reads, &writes);
            assert!(!result.is_valid());
            assert_eq!(result.conflict_count(), 1);
        }

        #[test]
        fn test_validate_json_paths_different_documents() {
            let key1 = create_json_key();
            let key2 = create_json_key();

            let reads = vec![];
            let writes = vec![
                JsonPatchEntry::new(
                    key1,
                    JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(1).into()),
                    2,
                ),
                JsonPatchEntry::new(
                    key2,
                    JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(2).into()),
                    3,
                ),
            ];

            // Same path but different documents - no conflict
            let result = validate_json_paths(&reads, &writes);
            assert!(result.is_valid());
        }

        #[test]
        fn test_validate_json_paths_read_write_same_path_allowed() {
            let key = create_json_key();

            // Read and write to the same path is allowed (read-your-writes)
            let reads = vec![JsonPathRead::new(key.clone(), "foo".parse().unwrap(), 1)];
            let writes = vec![JsonPatchEntry::new(
                key,
                JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(1).into()),
                2,
            )];

            let result = validate_json_paths(&reads, &writes);
            assert!(result.is_valid()); // No conflict - this is allowed
        }

        #[test]
        fn test_validate_json_paths_different_array_indices() {
            let key = create_json_key();

            let reads = vec![];
            let writes = vec![
                JsonPatchEntry::new(
                    key.clone(),
                    JsonPatch::set_at("items[0]".parse().unwrap(), serde_json::json!(1).into()),
                    2,
                ),
                JsonPatchEntry::new(
                    key.clone(),
                    JsonPatch::set_at("items[1]".parse().unwrap(), serde_json::json!(2).into()),
                    3,
                ),
            ];

            // Different array indices don't conflict
            let result = validate_json_paths(&reads, &writes);
            assert!(result.is_valid());
        }

        #[test]
        fn test_json_path_write_write_conflict_creation() {
            let key = create_json_key();
            let path1: JsonPath = "foo".parse().unwrap();
            let path2: JsonPath = "foo.bar".parse().unwrap();

            let conflict = ConflictType::JsonPathWriteWriteConflict {
                key: key.clone(),
                path1: path1.clone(),
                path2: path2.clone(),
            };

            match conflict {
                ConflictType::JsonPathWriteWriteConflict {
                    key: k,
                    path1: p1,
                    path2: p2,
                } => {
                    assert_eq!(k, key);
                    assert_eq!(p1, path1);
                    assert_eq!(p2, path2);
                }
                _ => panic!("Wrong conflict type"),
            }
        }
    }
}
