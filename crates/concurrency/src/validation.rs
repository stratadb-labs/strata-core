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

use in_mem_core::types::Key;

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
}
