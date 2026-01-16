//! JSON conflict detection for region-based concurrency control.
//!
//! This module provides conflict detection for JSON operations within transactions.
//! Two JSON operations conflict if their paths overlap (one is ancestor, descendant, or equal).
//!
//! # Conflict Types
//!
//! - **Read-Write Conflict**: A read at path X conflicts with a write at path Y if X.overlaps(Y)
//! - **Write-Write Conflict**: Two writes at paths X and Y conflict if X.overlaps(Y)
//! - **Version Mismatch**: The document version changed since transaction start

use std::collections::HashMap;

use in_mem_core::json::JsonPath;
use in_mem_core::types::Key;

use crate::transaction::{JsonPatchEntry, JsonPathRead};

/// Result of conflict detection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResult {
    /// No conflict detected
    NoConflict,
    /// Read-write conflict detected
    ReadWriteConflict {
        /// The document key where the conflict occurred
        key: Key,
        /// The path that was read
        read_path: JsonPath,
        /// The path that was written (conflicts with read_path)
        write_path: JsonPath,
    },
    /// Write-write conflict detected
    WriteWriteConflict {
        /// The document key where the conflict occurred
        key: Key,
        /// The first conflicting write path
        path1: JsonPath,
        /// The second conflicting write path
        path2: JsonPath,
    },
    /// Version mismatch (stale read)
    VersionMismatch {
        /// The document key where the version mismatch occurred
        key: Key,
        /// The version expected (from snapshot)
        expected: u64,
        /// The version found (current)
        found: u64,
    },
}

/// Error type for JSON conflicts
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum JsonConflictError {
    /// A read-write conflict was detected
    #[error("read-write conflict on {key:?}: read at {read_path}, write at {write_path}")]
    ReadWriteConflict {
        /// The document key where the conflict occurred
        key: Key,
        /// The path that was read
        read_path: JsonPath,
        /// The path that was written (conflicts with read_path)
        write_path: JsonPath,
    },
    /// A write-write conflict was detected
    #[error("write-write conflict on {key:?}: writes at {path1} and {path2}")]
    WriteWriteConflict {
        /// The document key where the conflict occurred
        key: Key,
        /// The first conflicting write path
        path1: JsonPath,
        /// The second conflicting write path
        path2: JsonPath,
    },
    /// A version mismatch was detected (stale read)
    #[error("version mismatch on {key:?}: expected {expected}, found {found}")]
    VersionMismatch {
        /// The document key where the version mismatch occurred
        key: Key,
        /// The version expected (from snapshot)
        expected: u64,
        /// The version found (current)
        found: u64,
    },
}

impl From<ConflictResult> for Option<JsonConflictError> {
    fn from(result: ConflictResult) -> Self {
        match result {
            ConflictResult::NoConflict => None,
            ConflictResult::ReadWriteConflict {
                key,
                read_path,
                write_path,
            } => Some(JsonConflictError::ReadWriteConflict {
                key,
                read_path,
                write_path,
            }),
            ConflictResult::WriteWriteConflict { key, path1, path2 } => {
                Some(JsonConflictError::WriteWriteConflict { key, path1, path2 })
            }
            ConflictResult::VersionMismatch {
                key,
                expected,
                found,
            } => Some(JsonConflictError::VersionMismatch {
                key,
                expected,
                found,
            }),
        }
    }
}

/// Check for read-write conflicts in a transaction
///
/// A read-write conflict occurs when:
/// - A path was read AND
/// - A write occurred at an overlapping path in the same document
///
/// # Arguments
///
/// * `reads` - List of JSON paths that were read during the transaction
/// * `writes` - List of JSON patches to be applied
///
/// # Returns
///
/// A vector of all detected read-write conflicts
pub fn check_read_write_conflicts(
    reads: &[JsonPathRead],
    writes: &[JsonPatchEntry],
) -> Vec<ConflictResult> {
    let mut conflicts = Vec::new();

    for read in reads {
        for write in writes {
            // Same document?
            if read.key != write.key {
                continue;
            }

            // Paths overlap?
            if read.path.overlaps(write.patch.path()) {
                conflicts.push(ConflictResult::ReadWriteConflict {
                    key: read.key.clone(),
                    read_path: read.path.clone(),
                    write_path: write.patch.path().clone(),
                });
            }
        }
    }

    conflicts
}

/// Find the first read-write conflict (for fast failure)
///
/// This is more efficient than `check_read_write_conflicts` when you only
/// need to know if any conflict exists, as it returns early.
///
/// # Arguments
///
/// * `reads` - List of JSON paths that were read during the transaction
/// * `writes` - List of JSON patches to be applied
///
/// # Returns
///
/// The first detected read-write conflict, or None if no conflicts exist
pub fn find_first_read_write_conflict(
    reads: &[JsonPathRead],
    writes: &[JsonPatchEntry],
) -> Option<ConflictResult> {
    for read in reads {
        for write in writes {
            if read.key == write.key && read.path.overlaps(write.patch.path()) {
                return Some(ConflictResult::ReadWriteConflict {
                    key: read.key.clone(),
                    read_path: read.path.clone(),
                    write_path: write.patch.path().clone(),
                });
            }
        }
    }
    None
}

/// Check for write-write conflicts in a transaction
///
/// A write-write conflict occurs when:
/// - Two writes target the same document AND
/// - The write paths overlap
///
/// # Arguments
///
/// * `writes` - List of JSON patches to be applied
///
/// # Returns
///
/// A vector of all detected write-write conflicts
pub fn check_write_write_conflicts(writes: &[JsonPatchEntry]) -> Vec<ConflictResult> {
    let mut conflicts = Vec::new();

    for (i, w1) in writes.iter().enumerate() {
        for w2 in writes.iter().skip(i + 1) {
            // Same document?
            if w1.key != w2.key {
                continue;
            }

            // Paths overlap?
            if w1.patch.path().overlaps(w2.patch.path()) {
                conflicts.push(ConflictResult::WriteWriteConflict {
                    key: w1.key.clone(),
                    path1: w1.patch.path().clone(),
                    path2: w2.patch.path().clone(),
                });
            }
        }
    }

    conflicts
}

/// Find the first write-write conflict (for fast failure)
///
/// # Arguments
///
/// * `writes` - List of JSON patches to be applied
///
/// # Returns
///
/// The first detected write-write conflict, or None if no conflicts exist
pub fn find_first_write_write_conflict(writes: &[JsonPatchEntry]) -> Option<ConflictResult> {
    for (i, w1) in writes.iter().enumerate() {
        for w2 in writes.iter().skip(i + 1) {
            if w1.key == w2.key && w1.patch.path().overlaps(w2.patch.path()) {
                return Some(ConflictResult::WriteWriteConflict {
                    key: w1.key.clone(),
                    path1: w1.patch.path().clone(),
                    path2: w2.patch.path().clone(),
                });
            }
        }
    }
    None
}

/// Check for version conflicts (stale reads)
///
/// A version conflict occurs when the document version at commit time
/// differs from the version captured at transaction start.
///
/// # Arguments
///
/// * `snapshot_versions` - Versions of documents when the transaction started
/// * `current_versions` - Current versions of documents
///
/// # Returns
///
/// A vector of all detected version mismatches
pub fn check_version_conflicts(
    snapshot_versions: &HashMap<Key, u64>,
    current_versions: &HashMap<Key, u64>,
) -> Vec<ConflictResult> {
    let mut conflicts = Vec::new();

    for (key, snapshot_version) in snapshot_versions {
        let current = current_versions.get(key).copied().unwrap_or(0);
        if current != *snapshot_version {
            conflicts.push(ConflictResult::VersionMismatch {
                key: key.clone(),
                expected: *snapshot_version,
                found: current,
            });
        }
    }

    conflicts
}

/// Find the first version conflict (for fast failure)
///
/// # Arguments
///
/// * `snapshot_versions` - Versions of documents when the transaction started
/// * `current_versions` - Current versions of documents
///
/// # Returns
///
/// The first detected version mismatch, or None if no conflicts exist
pub fn find_first_version_conflict(
    snapshot_versions: &HashMap<Key, u64>,
    current_versions: &HashMap<Key, u64>,
) -> Option<ConflictResult> {
    for (key, snapshot_version) in snapshot_versions {
        let current = current_versions.get(key).copied().unwrap_or(0);
        if current != *snapshot_version {
            return Some(ConflictResult::VersionMismatch {
                key: key.clone(),
                expected: *snapshot_version,
                found: current,
            });
        }
    }
    None
}

/// Comprehensive conflict check
///
/// Checks for all types of conflicts in the following order (fastest to detect first):
/// 1. Version conflicts (stale reads)
/// 2. Write-write conflicts
/// 3. Read-write conflicts
///
/// # Arguments
///
/// * `reads` - List of JSON paths that were read during the transaction
/// * `writes` - List of JSON patches to be applied
/// * `snapshot_versions` - Versions of documents when the transaction started
/// * `current_versions` - Current versions of documents
///
/// # Returns
///
/// Ok(()) if no conflicts, Err with the first conflict found otherwise
pub fn check_all_conflicts(
    reads: &[JsonPathRead],
    writes: &[JsonPatchEntry],
    snapshot_versions: &HashMap<Key, u64>,
    current_versions: &HashMap<Key, u64>,
) -> Result<(), JsonConflictError> {
    // 1. Check version conflicts first (fastest to detect)
    if let Some(conflict) = find_first_version_conflict(snapshot_versions, current_versions) {
        return Err(Option::<JsonConflictError>::from(conflict).unwrap());
    }

    // 2. Check write-write conflicts
    if let Some(conflict) = find_first_write_write_conflict(writes) {
        return Err(Option::<JsonConflictError>::from(conflict).unwrap());
    }

    // 3. Check read-write conflicts
    if let Some(conflict) = find_first_read_write_conflict(reads, writes) {
        return Err(Option::<JsonConflictError>::from(conflict).unwrap());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use in_mem_core::json::JsonPatch;
    use in_mem_core::types::{JsonDocId, Namespace, RunId};

    fn test_key() -> Key {
        Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new())
    }

    #[test]
    fn test_read_write_conflict_detection() {
        let key = test_key();

        let reads = vec![JsonPathRead::new(
            key.clone(),
            "foo.bar".parse().unwrap(),
            1,
        )];

        let writes = vec![
            // This write overlaps with the read (ancestor)
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(42).into()),
                2,
            ),
        ];

        let conflicts = check_read_write_conflicts(&reads, &writes);
        assert_eq!(conflicts.len(), 1);
        assert!(matches!(
            conflicts[0],
            ConflictResult::ReadWriteConflict { .. }
        ));
    }

    #[test]
    fn test_read_write_conflict_descendant() {
        let key = test_key();

        let reads = vec![JsonPathRead::new(key.clone(), "foo".parse().unwrap(), 1)];

        let writes = vec![
            // This write overlaps with the read (descendant)
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("foo.bar.baz".parse().unwrap(), serde_json::json!(42).into()),
                2,
            ),
        ];

        let conflicts = check_read_write_conflicts(&reads, &writes);
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn test_no_conflict_different_paths() {
        let key = test_key();

        let reads = vec![JsonPathRead::new(key.clone(), "foo".parse().unwrap(), 1)];

        let writes = vec![JsonPatchEntry::new(
            key.clone(),
            JsonPatch::set_at("bar".parse().unwrap(), serde_json::json!(42).into()),
            2,
        )];

        let conflicts = check_read_write_conflicts(&reads, &writes);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_no_conflict_different_documents() {
        let key1 = test_key();
        let key2 = test_key();

        let reads = vec![JsonPathRead::new(key1.clone(), "foo".parse().unwrap(), 1)];

        let writes = vec![
            // Same path but different document
            JsonPatchEntry::new(
                key2.clone(),
                JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(42).into()),
                2,
            ),
        ];

        let conflicts = check_read_write_conflicts(&reads, &writes);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_multiple_read_write_conflicts() {
        let key = test_key();

        let reads = vec![
            JsonPathRead::new(key.clone(), "a".parse().unwrap(), 1),
            JsonPathRead::new(key.clone(), "b".parse().unwrap(), 1),
        ];

        let writes = vec![
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("a.x".parse().unwrap(), serde_json::json!(1).into()),
                2,
            ),
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("b.y".parse().unwrap(), serde_json::json!(2).into()),
                2,
            ),
        ];

        let conflicts = check_read_write_conflicts(&reads, &writes);
        assert_eq!(conflicts.len(), 2);
    }

    #[test]
    fn test_find_first_read_write_conflict() {
        let key = test_key();

        let reads = vec![
            JsonPathRead::new(key.clone(), "a".parse().unwrap(), 1),
            JsonPathRead::new(key.clone(), "b".parse().unwrap(), 1),
        ];

        let writes = vec![
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("a.x".parse().unwrap(), serde_json::json!(1).into()),
                2,
            ),
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("b.y".parse().unwrap(), serde_json::json!(2).into()),
                2,
            ),
        ];

        let conflict = find_first_read_write_conflict(&reads, &writes);
        assert!(conflict.is_some());
        // Should return the first one found
        assert!(matches!(
            conflict.unwrap(),
            ConflictResult::ReadWriteConflict { .. }
        ));
    }

    #[test]
    fn test_write_write_conflict_detection() {
        let key = test_key();

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

        let conflicts = check_write_write_conflicts(&writes);
        assert_eq!(conflicts.len(), 1);
        assert!(matches!(
            conflicts[0],
            ConflictResult::WriteWriteConflict { .. }
        ));
    }

    #[test]
    fn test_no_write_conflict_disjoint_paths() {
        let key = test_key();

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
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("baz".parse().unwrap(), serde_json::json!(3).into()),
                4,
            ),
        ];

        let conflicts = check_write_write_conflicts(&writes);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_write_write_different_documents() {
        let key1 = test_key();
        let key2 = test_key();

        let writes = vec![
            JsonPatchEntry::new(
                key1.clone(),
                JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(1).into()),
                2,
            ),
            JsonPatchEntry::new(
                key2.clone(),
                JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(2).into()),
                3,
            ),
        ];

        // Same path but different documents - no conflict
        let conflicts = check_write_write_conflicts(&writes);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_version_conflict_detection() {
        let key = test_key();

        let mut snapshot = HashMap::new();
        snapshot.insert(key.clone(), 5);

        let mut current = HashMap::new();
        current.insert(key.clone(), 7); // Version changed

        let conflicts = check_version_conflicts(&snapshot, &current);
        assert_eq!(conflicts.len(), 1);
        match &conflicts[0] {
            ConflictResult::VersionMismatch {
                expected, found, ..
            } => {
                assert_eq!(*expected, 5);
                assert_eq!(*found, 7);
            }
            _ => panic!("Expected VersionMismatch"),
        }
    }

    #[test]
    fn test_version_conflict_document_deleted() {
        let key = test_key();

        let mut snapshot = HashMap::new();
        snapshot.insert(key.clone(), 5);

        let current = HashMap::new(); // Document no longer exists

        let conflicts = check_version_conflicts(&snapshot, &current);
        assert_eq!(conflicts.len(), 1);
        match &conflicts[0] {
            ConflictResult::VersionMismatch {
                expected, found, ..
            } => {
                assert_eq!(*expected, 5);
                assert_eq!(*found, 0); // Missing = version 0
            }
            _ => panic!("Expected VersionMismatch"),
        }
    }

    #[test]
    fn test_no_version_conflict() {
        let key = test_key();

        let mut snapshot = HashMap::new();
        snapshot.insert(key.clone(), 5);

        let mut current = HashMap::new();
        current.insert(key.clone(), 5); // Version unchanged

        let conflicts = check_version_conflicts(&snapshot, &current);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_check_all_conflicts_version_first() {
        let key = test_key();

        let reads = vec![JsonPathRead::new(key.clone(), "foo".parse().unwrap(), 5)];

        let writes = vec![
            // This also conflicts (same path)
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(1).into()),
                6,
            ),
        ];

        let mut snapshot = HashMap::new();
        snapshot.insert(key.clone(), 5);

        let mut current = HashMap::new();
        current.insert(key.clone(), 7); // Version changed - should fail first

        let result = check_all_conflicts(&reads, &writes, &snapshot, &current);

        // Version conflict should be detected first
        assert!(matches!(
            result,
            Err(JsonConflictError::VersionMismatch { .. })
        ));
    }

    #[test]
    fn test_check_all_conflicts_write_write_second() {
        let key = test_key();

        let reads = vec![JsonPathRead::new(key.clone(), "a".parse().unwrap(), 5)];

        let writes = vec![
            // These two overlap
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("foo".parse().unwrap(), serde_json::json!(1).into()),
                6,
            ),
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("foo.bar".parse().unwrap(), serde_json::json!(2).into()),
                7,
            ),
        ];

        let mut snapshot = HashMap::new();
        snapshot.insert(key.clone(), 5);

        let mut current = HashMap::new();
        current.insert(key.clone(), 5); // Version unchanged

        let result = check_all_conflicts(&reads, &writes, &snapshot, &current);

        // Write-write conflict should be detected
        assert!(matches!(
            result,
            Err(JsonConflictError::WriteWriteConflict { .. })
        ));
    }

    #[test]
    fn test_check_all_conflicts_read_write_third() {
        let key = test_key();

        let reads = vec![JsonPathRead::new(key.clone(), "foo".parse().unwrap(), 5)];

        let writes = vec![
            // This overlaps with the read
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("foo.bar".parse().unwrap(), serde_json::json!(1).into()),
                6,
            ),
        ];

        let mut snapshot = HashMap::new();
        snapshot.insert(key.clone(), 5);

        let mut current = HashMap::new();
        current.insert(key.clone(), 5); // Version unchanged

        let result = check_all_conflicts(&reads, &writes, &snapshot, &current);

        // Read-write conflict should be detected
        assert!(matches!(
            result,
            Err(JsonConflictError::ReadWriteConflict { .. })
        ));
    }

    #[test]
    fn test_check_all_conflicts_success() {
        let key = test_key();

        let reads = vec![JsonPathRead::new(key.clone(), "a".parse().unwrap(), 5)];

        let writes = vec![
            // No overlap with read
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("b".parse().unwrap(), serde_json::json!(1).into()),
                6,
            ),
            JsonPatchEntry::new(
                key.clone(),
                JsonPatch::set_at("c".parse().unwrap(), serde_json::json!(2).into()),
                7,
            ),
        ];

        let mut snapshot = HashMap::new();
        snapshot.insert(key.clone(), 5);

        let mut current = HashMap::new();
        current.insert(key.clone(), 5); // Version unchanged

        let result = check_all_conflicts(&reads, &writes, &snapshot, &current);

        assert!(result.is_ok());
    }

    #[test]
    fn test_json_conflict_error_display() {
        let key = test_key();

        let err = JsonConflictError::ReadWriteConflict {
            key: key.clone(),
            read_path: "foo".parse().unwrap(),
            write_path: "foo.bar".parse().unwrap(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("read-write conflict"));
        assert!(msg.contains("foo"));

        let err = JsonConflictError::WriteWriteConflict {
            key: key.clone(),
            path1: "foo".parse().unwrap(),
            path2: "foo.bar".parse().unwrap(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("write-write conflict"));

        let err = JsonConflictError::VersionMismatch {
            key,
            expected: 5,
            found: 7,
        };
        let msg = format!("{}", err);
        assert!(msg.contains("version mismatch"));
        assert!(msg.contains("5"));
        assert!(msg.contains("7"));
    }
}
