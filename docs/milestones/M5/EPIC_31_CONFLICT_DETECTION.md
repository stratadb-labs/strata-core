# Epic 31: Conflict Detection

**Goal**: Implement region-based conflict detection for JSON

**Dependencies**: Epic 30 complete

**GitHub Issue**: #261

---

## Scope

- Path overlap detection
- Read-write conflict check
- Write-write conflict check
- Integration with commit flow

---

## Critical Invariants

**Region-Based Conflict Detection**: Two JSON operations conflict if and only if their paths overlap.

Overlap means:
- **Equal**: `$.foo` and `$.foo`
- **Ancestor**: `$.foo` and `$.foo.bar`
- **Descendant**: `$.foo.bar` and `$.foo`

Non-overlapping paths do NOT conflict:
- `$.foo` and `$.bar` - different keys
- `$.items[0]` and `$.items[1]` - different indices

---

## User Stories

| Story | Description | Priority | GitHub Issue |
|-------|-------------|----------|--------------|
| #249 | Path Overlap Detection | CRITICAL | #287 |
| #250 | Read-Write Conflict Check | CRITICAL | #288 |
| #251 | Write-Write Conflict Check | CRITICAL | #289 |
| #252 | Conflict Integration with Commit | HIGH | #290 |

---

## Story #249: Path Overlap Detection

**File**: `crates/core/src/json_types.rs`

**Deliverable**: Comprehensive path overlap detection

### Implementation

```rust
impl JsonPath {
    /// Check if two paths overlap
    ///
    /// Paths overlap if:
    /// - They are equal
    /// - One is an ancestor of the other
    /// - One is a descendant of the other
    ///
    /// # Examples
    ///
    /// ```
    /// // Equal paths overlap
    /// assert!(JsonPath::parse("foo").unwrap().overlaps(&JsonPath::parse("foo").unwrap()));
    ///
    /// // Ancestor/descendant overlap
    /// assert!(JsonPath::parse("foo").unwrap().overlaps(&JsonPath::parse("foo.bar").unwrap()));
    /// assert!(JsonPath::parse("foo.bar").unwrap().overlaps(&JsonPath::parse("foo").unwrap()));
    ///
    /// // Root overlaps with everything
    /// assert!(JsonPath::root().overlaps(&JsonPath::parse("foo.bar").unwrap()));
    ///
    /// // Disjoint paths don't overlap
    /// assert!(!JsonPath::parse("foo").unwrap().overlaps(&JsonPath::parse("bar").unwrap()));
    /// assert!(!JsonPath::parse("items[0]").unwrap().overlaps(&JsonPath::parse("items[1]").unwrap()));
    /// ```
    pub fn overlaps(&self, other: &JsonPath) -> bool {
        let min_len = self.segments.len().min(other.segments.len());

        // Check if common prefix matches
        for i in 0..min_len {
            if self.segments[i] != other.segments[i] {
                return false; // Disjoint paths
            }
        }

        // If we get here, one path is a prefix of the other (or equal)
        true
    }

    /// Check if this path is a strict ancestor of another (not equal)
    pub fn is_strict_ancestor_of(&self, other: &JsonPath) -> bool {
        self.segments.len() < other.segments.len() && self.is_ancestor_of(other)
    }

    /// Check if this path is a strict descendant of another (not equal)
    pub fn is_strict_descendant_of(&self, other: &JsonPath) -> bool {
        other.is_strict_ancestor_of(self)
    }

    /// Get the common ancestor path of two paths
    pub fn common_ancestor(&self, other: &JsonPath) -> JsonPath {
        let mut common = Vec::new();

        for (a, b) in self.segments.iter().zip(other.segments.iter()) {
            if a == b {
                common.push(a.clone());
            } else {
                break;
            }
        }

        JsonPath { segments: common }
    }

    /// Check if a path is affected by an operation at another path
    ///
    /// A path is affected if:
    /// - It overlaps with the operation path
    /// - Array indices may shift if an earlier index is modified
    pub fn is_affected_by(&self, operation_path: &JsonPath) -> bool {
        // Basic overlap check
        if self.overlaps(operation_path) {
            return true;
        }

        // Check for array index shifting
        // If operation is on items[0], then items[1], items[2], etc. are affected
        if let Some(common) = self.common_prefix_with_diverging_index(operation_path) {
            // Both paths have an array index at the divergence point
            // The path is affected if the operation index is before our index
            return common.0 < common.1;
        }

        false
    }

    /// Find diverging array indices if paths share a common prefix up to array access
    fn common_prefix_with_diverging_index(&self, other: &JsonPath) -> Option<(usize, usize)> {
        let min_len = self.segments.len().min(other.segments.len());

        for i in 0..min_len {
            match (&self.segments[i], &other.segments[i]) {
                (PathSegment::Index(a), PathSegment::Index(b)) if a != b => {
                    // Check if all previous segments match
                    if self.segments[..i] == other.segments[..i] {
                        return Some((*b, *a)); // (operation index, our index)
                    }
                }
                (a, b) if a != b => {
                    return None; // Diverged on non-index
                }
                _ => continue,
            }
        }

        None
    }
}
```

### Acceptance Criteria

- [ ] Equal paths overlap
- [ ] Ancestor/descendant paths overlap
- [ ] Disjoint paths don't overlap
- [ ] Different array indices don't overlap
- [ ] Root overlaps with everything
- [ ] common_ancestor() returns correct path

### Testing

```rust
#[test]
fn test_path_overlap_equal() {
    let p1 = JsonPath::parse("foo.bar").unwrap();
    let p2 = JsonPath::parse("foo.bar").unwrap();
    assert!(p1.overlaps(&p2));
    assert!(p2.overlaps(&p1)); // Symmetric
}

#[test]
fn test_path_overlap_ancestor_descendant() {
    let ancestor = JsonPath::parse("foo").unwrap();
    let descendant = JsonPath::parse("foo.bar.baz").unwrap();

    assert!(ancestor.overlaps(&descendant));
    assert!(descendant.overlaps(&ancestor));
}

#[test]
fn test_path_overlap_root() {
    let root = JsonPath::root();
    let deep = JsonPath::parse("a.b.c.d.e").unwrap();

    assert!(root.overlaps(&deep));
    assert!(deep.overlaps(&root));
    assert!(root.overlaps(&root));
}

#[test]
fn test_path_no_overlap_disjoint() {
    let p1 = JsonPath::parse("foo").unwrap();
    let p2 = JsonPath::parse("bar").unwrap();
    assert!(!p1.overlaps(&p2));

    let p3 = JsonPath::parse("foo.bar").unwrap();
    let p4 = JsonPath::parse("foo.baz").unwrap();
    assert!(!p3.overlaps(&p4));
}

#[test]
fn test_path_no_overlap_different_indices() {
    let p1 = JsonPath::parse("items[0]").unwrap();
    let p2 = JsonPath::parse("items[1]").unwrap();
    assert!(!p1.overlaps(&p2));

    let p3 = JsonPath::parse("items[0].name").unwrap();
    let p4 = JsonPath::parse("items[1].name").unwrap();
    assert!(!p3.overlaps(&p4));
}

#[test]
fn test_path_common_ancestor() {
    let p1 = JsonPath::parse("foo.bar.baz").unwrap();
    let p2 = JsonPath::parse("foo.bar.qux").unwrap();

    let common = p1.common_ancestor(&p2);
    assert_eq!(common.to_string(), "$.foo.bar");
}

#[test]
fn test_path_strict_ancestor() {
    let p1 = JsonPath::parse("foo").unwrap();
    let p2 = JsonPath::parse("foo.bar").unwrap();

    assert!(p1.is_strict_ancestor_of(&p2));
    assert!(!p2.is_strict_ancestor_of(&p1));
    assert!(!p1.is_strict_ancestor_of(&p1)); // Not strict if equal
}
```

---

## Story #250: Read-Write Conflict Check

**File**: `crates/concurrency/src/conflict.rs` (NEW)

**Deliverable**: Detect read-write conflicts

### Implementation

```rust
use in_mem_core::json_types::JsonPath;
use in_mem_core::types::Key;
use super::transaction::{JsonPathRead, JsonPatchEntry};

/// Result of conflict detection
#[derive(Debug, Clone)]
pub enum ConflictResult {
    /// No conflict detected
    NoConflict,
    /// Read-write conflict detected
    ReadWriteConflict {
        key: Key,
        read_path: JsonPath,
        write_path: JsonPath,
    },
    /// Write-write conflict detected
    WriteWriteConflict {
        key: Key,
        path1: JsonPath,
        path2: JsonPath,
    },
    /// Version mismatch (stale read)
    VersionMismatch {
        key: Key,
        expected: u64,
        found: u64,
    },
}

/// Check for read-write conflicts in a transaction
///
/// A read-write conflict occurs when:
/// - A path was read AND
/// - A write occurred at an overlapping path in the same document
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

#[derive(Debug, Clone, thiserror::Error)]
pub enum JsonConflictError {
    #[error("read-write conflict on {key:?}: read at {read_path}, write at {write_path}")]
    ReadWriteConflict {
        key: Key,
        read_path: JsonPath,
        write_path: JsonPath,
    },
    #[error("write-write conflict on {key:?}: writes at {path1} and {path2}")]
    WriteWriteConflict {
        key: Key,
        path1: JsonPath,
        path2: JsonPath,
    },
    #[error("version mismatch on {key:?}: expected {expected}, found {found}")]
    VersionMismatch {
        key: Key,
        expected: u64,
        found: u64,
    },
}

impl From<ConflictResult> for Option<JsonConflictError> {
    fn from(result: ConflictResult) -> Self {
        match result {
            ConflictResult::NoConflict => None,
            ConflictResult::ReadWriteConflict { key, read_path, write_path } => {
                Some(JsonConflictError::ReadWriteConflict { key, read_path, write_path })
            }
            ConflictResult::WriteWriteConflict { key, path1, path2 } => {
                Some(JsonConflictError::WriteWriteConflict { key, path1, path2 })
            }
            ConflictResult::VersionMismatch { key, expected, found } => {
                Some(JsonConflictError::VersionMismatch { key, expected, found })
            }
        }
    }
}
```

### Acceptance Criteria

- [ ] Detects overlapping read and write paths
- [ ] Only same-document conflicts are detected
- [ ] Returns detailed error info with paths
- [ ] find_first_read_write_conflict() provides fast failure path

### Testing

```rust
#[test]
fn test_read_write_conflict_detection() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let reads = vec![
        JsonPathRead::new(key.clone(), JsonPath::parse("foo.bar").unwrap(), 1),
    ];

    let writes = vec![
        // This write overlaps with the read (ancestor)
        JsonPatchEntry::new(
            key.clone(),
            JsonPatch::set(JsonPath::parse("foo").unwrap(), 42),
            2,
        ),
    ];

    let conflicts = check_read_write_conflicts(&reads, &writes);
    assert_eq!(conflicts.len(), 1);
    assert!(matches!(conflicts[0], ConflictResult::ReadWriteConflict { .. }));
}

#[test]
fn test_no_conflict_different_paths() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let reads = vec![
        JsonPathRead::new(key.clone(), JsonPath::parse("foo").unwrap(), 1),
    ];

    let writes = vec![
        JsonPatchEntry::new(
            key.clone(),
            JsonPatch::set(JsonPath::parse("bar").unwrap(), 42),
            2,
        ),
    ];

    let conflicts = check_read_write_conflicts(&reads, &writes);
    assert!(conflicts.is_empty());
}

#[test]
fn test_no_conflict_different_documents() {
    let key1 = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());
    let key2 = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let reads = vec![
        JsonPathRead::new(key1.clone(), JsonPath::parse("foo").unwrap(), 1),
    ];

    let writes = vec![
        // Same path but different document
        JsonPatchEntry::new(
            key2.clone(),
            JsonPatch::set(JsonPath::parse("foo").unwrap(), 42),
            2,
        ),
    ];

    let conflicts = check_read_write_conflicts(&reads, &writes);
    assert!(conflicts.is_empty());
}

#[test]
fn test_multiple_conflicts() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let reads = vec![
        JsonPathRead::new(key.clone(), JsonPath::parse("a").unwrap(), 1),
        JsonPathRead::new(key.clone(), JsonPath::parse("b").unwrap(), 1),
    ];

    let writes = vec![
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("a.x").unwrap(), 1), 2),
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("b.y").unwrap(), 2), 2),
    ];

    let conflicts = check_read_write_conflicts(&reads, &writes);
    assert_eq!(conflicts.len(), 2);
}
```

---

## Story #251: Write-Write Conflict Check

**File**: `crates/concurrency/src/conflict.rs`

**Deliverable**: Detect write-write conflicts

### Implementation

```rust
/// Check for write-write conflicts in a transaction
///
/// A write-write conflict occurs when:
/// - Two writes target the same document AND
/// - The write paths overlap
pub fn check_write_write_conflicts(
    writes: &[JsonPatchEntry],
) -> Vec<ConflictResult> {
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
pub fn find_first_write_write_conflict(
    writes: &[JsonPatchEntry],
) -> Option<ConflictResult> {
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

/// Comprehensive conflict check
pub fn check_all_conflicts(
    reads: &[JsonPathRead],
    writes: &[JsonPatchEntry],
    snapshot_versions: &HashMap<Key, u64>,
    current_versions: &HashMap<Key, u64>,
) -> Result<(), JsonConflictError> {
    // 1. Check version conflicts first (fastest to detect)
    if let Some(conflict) = check_version_conflicts(snapshot_versions, current_versions).first() {
        return Err(conflict.clone().into().unwrap());
    }

    // 2. Check write-write conflicts
    if let Some(conflict) = find_first_write_write_conflict(writes) {
        return Err(conflict.into().unwrap());
    }

    // 3. Check read-write conflicts
    if let Some(conflict) = find_first_read_write_conflict(reads, writes) {
        return Err(conflict.into().unwrap());
    }

    Ok(())
}
```

### Acceptance Criteria

- [ ] Detects overlapping write paths
- [ ] Only same-document conflicts are detected
- [ ] Detects version mismatches (stale reads)
- [ ] check_all_conflicts() provides comprehensive validation
- [ ] Fast failure path with find_first_* functions

### Testing

```rust
#[test]
fn test_write_write_conflict_detection() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let writes = vec![
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("foo").unwrap(), 1), 2),
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("foo.bar").unwrap(), 2), 3),
    ];

    let conflicts = check_write_write_conflicts(&writes);
    assert_eq!(conflicts.len(), 1);
    assert!(matches!(conflicts[0], ConflictResult::WriteWriteConflict { .. }));
}

#[test]
fn test_no_write_conflict_disjoint_paths() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let writes = vec![
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("foo").unwrap(), 1), 2),
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("bar").unwrap(), 2), 3),
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("baz").unwrap(), 3), 4),
    ];

    let conflicts = check_write_write_conflicts(&writes);
    assert!(conflicts.is_empty());
}

#[test]
fn test_version_conflict_detection() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let mut snapshot = HashMap::new();
    snapshot.insert(key.clone(), 5);

    let mut current = HashMap::new();
    current.insert(key.clone(), 7); // Version changed

    let conflicts = check_version_conflicts(&snapshot, &current);
    assert_eq!(conflicts.len(), 1);
    match &conflicts[0] {
        ConflictResult::VersionMismatch { expected, found, .. } => {
            assert_eq!(*expected, 5);
            assert_eq!(*found, 7);
        }
        _ => panic!("Expected VersionMismatch"),
    }
}

#[test]
fn test_check_all_conflicts_order() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let reads = vec![
        JsonPathRead::new(key.clone(), JsonPath::parse("foo").unwrap(), 5),
    ];

    let writes = vec![
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("foo").unwrap(), 1), 6),
    ];

    let mut snapshot = HashMap::new();
    snapshot.insert(key.clone(), 5);

    let mut current = HashMap::new();
    current.insert(key.clone(), 7); // Version changed - should fail first

    let result = check_all_conflicts(&reads, &writes, &snapshot, &current);

    // Version conflict should be detected first
    assert!(matches!(result, Err(JsonConflictError::VersionMismatch { .. })));
}
```

---

## Story #252: Conflict Integration with Commit

**File**: `crates/engine/src/database.rs`

**Deliverable**: Integrate conflict detection into commit flow

### Implementation

```rust
use in_mem_concurrency::conflict::{check_all_conflicts, JsonConflictError};

impl Database {
    /// Validate all JSON conflicts before commit
    pub fn validate_json_transaction(
        &self,
        ctx: &TransactionContext,
    ) -> Result<(), TransactionError> {
        if !ctx.has_json_ops() {
            return Ok(());
        }

        // Get current versions for all documents in snapshot
        let current_versions = self.get_current_json_versions(ctx)?;

        // Run comprehensive conflict check
        let reads = ctx.json_reads().map(|v| v.as_slice()).unwrap_or(&[]);
        let writes = ctx.json_writes().map(|v| v.as_slice()).unwrap_or(&[]);
        let snapshot = ctx.json_snapshot_versions().cloned().unwrap_or_default();

        check_all_conflicts(reads, writes, &snapshot, &current_versions)
            .map_err(TransactionError::JsonConflict)?;

        Ok(())
    }

    /// Get current versions for documents in transaction
    fn get_current_json_versions(
        &self,
        ctx: &TransactionContext,
    ) -> Result<HashMap<Key, u64>, Error> {
        let mut versions = HashMap::new();

        if let Some(snapshot) = ctx.json_snapshot_versions() {
            for key in snapshot.keys() {
                let version = match self.storage().get(key)? {
                    Some(vv) => {
                        let doc = JsonStore::deserialize_doc(&vv.value)?;
                        doc.version
                    }
                    None => 0, // Document was deleted
                };
                versions.insert(key.clone(), version);
            }
        }

        Ok(versions)
    }

    /// Commit transaction with JSON support
    pub fn commit_with_json(&self, ctx: TransactionContext) -> Result<(), TransactionError> {
        // Validate JSON conflicts
        self.validate_json_transaction(&ctx)?;

        // Write WAL entries for JSON operations
        if let Some(writes) = ctx.json_writes() {
            for write in writes {
                let wal_entry = match &write.patch {
                    JsonPatch::Set { path, value } => JsonWalEntry::Set {
                        key: write.key.clone(),
                        path: path.clone(),
                        value: value.clone(),
                        version: write.resulting_version,
                    },
                    JsonPatch::Delete { path } => JsonWalEntry::Delete {
                        key: write.key.clone(),
                        path: path.clone(),
                        version: write.resulting_version,
                    },
                };
                self.wal().append(&WALEntry::Json(wal_entry))?;
            }
        }

        // Commit all primitives
        self.commit_storage(&ctx)?;

        Ok(())
    }
}

// Add JSON conflict to TransactionError
impl From<JsonConflictError> for TransactionError {
    fn from(e: JsonConflictError) -> Self {
        TransactionError::JsonConflict(e)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    // ... existing variants ...

    #[error("JSON conflict: {0}")]
    JsonConflict(#[from] JsonConflictError),
}
```

### Acceptance Criteria

- [ ] All conflict types checked before commit
- [ ] Version conflicts checked first (fastest)
- [ ] Conflicts abort entire transaction
- [ ] JSON WAL entries written on commit
- [ ] JSON commits with other primitives atomically

### Testing

```rust
#[test]
fn test_commit_detects_stale_read() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let json = JsonStore::new(db.clone());
    json.create(&run_id, &doc_id, JsonValue::from(1)).unwrap();

    // Start transaction and read
    let result = std::thread::scope(|s| {
        let db = db.clone();
        let run_id = run_id.clone();
        let doc_id = doc_id.clone();

        // Thread 1: Start transaction, read, then try to commit
        let handle = s.spawn(move || {
            db.transaction(run_id, |txn| {
                let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

                // Read the document
                txn.json_get(&key, &JsonPath::root())?;

                // Simulate delay
                std::thread::sleep(std::time::Duration::from_millis(100));

                // Try to commit (should fail if concurrent modification)
                Ok(())
            })
        });

        // Thread 2: Modify document while thread 1 is in transaction
        std::thread::sleep(std::time::Duration::from_millis(50));
        json.set(&run_id, &doc_id, &JsonPath::root(), JsonValue::from(2)).unwrap();

        handle.join().unwrap()
    });

    // Transaction should fail with stale read
    assert!(matches!(result, Err(TransactionError::JsonConflict(
        JsonConflictError::VersionMismatch { .. }
    ))));
}

#[test]
fn test_commit_detects_write_write_conflict() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    let json = JsonStore::new(db.clone());
    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    let result = db.transaction(run_id, |txn| {
        // Two overlapping writes in same transaction
        txn.json_set(&key, &JsonPath::parse("foo").unwrap(), JsonValue::from(1))?;
        txn.json_set(&key, &JsonPath::parse("foo.bar").unwrap(), JsonValue::from(2))?;

        Ok(())
    });

    assert!(matches!(result, Err(TransactionError::JsonConflict(
        JsonConflictError::WriteWriteConflict { .. }
    ))));
}

#[test]
fn test_commit_success_no_conflict() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    let json = JsonStore::new(db.clone());
    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    // Non-overlapping operations should succeed
    let result = db.transaction(run_id, |txn| {
        txn.json_set(&key, &JsonPath::parse("a").unwrap(), JsonValue::from(1))?;
        txn.json_set(&key, &JsonPath::parse("b").unwrap(), JsonValue::from(2))?;
        txn.json_set(&key, &JsonPath::parse("c").unwrap(), JsonValue::from(3))?;

        Ok(())
    });

    assert!(result.is_ok());

    // Verify all values committed
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(1));
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("b").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(2));
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("c").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(3));
}

#[test]
fn test_conflict_aborts_entire_transaction() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    let json = JsonStore::new(db.clone());
    let kv = KVStore::new(db.clone());

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    // Transaction with conflict should rollback everything
    let result = db.transaction(run_id, |txn| {
        // KV operation
        let kv_key = Key::new_kv(Namespace::for_run(run_id), b"key");
        txn.put(kv_key, Value::from(42))?;

        // Conflicting JSON operations
        txn.json_set(&key, &JsonPath::parse("foo").unwrap(), JsonValue::from(1))?;
        txn.json_set(&key, &JsonPath::parse("foo.bar").unwrap(), JsonValue::from(2))?;

        Ok(())
    });

    assert!(result.is_err());

    // KV should also be rolled back
    assert!(kv.get(&run_id, b"key").unwrap().is_none());
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/json_types.rs` | MODIFY - Add overlap detection methods |
| `crates/concurrency/src/conflict.rs` | CREATE - Conflict detection functions |
| `crates/concurrency/src/lib.rs` | MODIFY - Export conflict module |
| `crates/engine/src/database.rs` | MODIFY - Integrate conflict detection |

---

## Success Criteria

- [ ] overlaps() correctly identifies ancestor/descendant/equal paths
- [ ] Read at path X conflicts with write at path Y if X.overlaps(Y)
- [ ] Write at path X conflicts with write at path Y if X.overlaps(Y)
- [ ] Conflict detection integrated into existing validation pipeline
- [ ] Version mismatch (stale read) detected and reported
- [ ] Conflicts abort entire transaction (including other primitives)
