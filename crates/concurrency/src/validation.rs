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
}
