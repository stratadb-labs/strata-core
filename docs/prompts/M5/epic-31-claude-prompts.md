# Epic 31: Conflict Detection - Implementation Prompts

**Epic Goal**: Implement region-based conflict detection for JSON

**GitHub Issue**: [#261](https://github.com/anibjoshi/in-mem/issues/261)
**Status**: Ready after Epic 30
**Dependencies**: Epic 30 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M5_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M5_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M5/EPIC_31_CONFLICT_DETECTION.md`
3. **Prompt Header**: `docs/prompts/M5/M5_PROMPT_HEADER.md` for the 6 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## CRITICAL: REGION-BASED CONFLICT DETECTION

**Two JSON operations conflict if and only if their paths overlap.**

Overlap means:
- **Equal**: `$.foo` and `$.foo`
- **Ancestor**: `$.foo` and `$.foo.bar`
- **Descendant**: `$.foo.bar` and `$.foo`

Non-overlapping paths do NOT conflict:
- `$.foo` and `$.bar` - different keys
- `$.items[0]` and `$.items[1]` - different indices

---

## Epic 31 Overview

### Scope
- Path overlap detection
- Read-write conflict check
- Write-write conflict check
- Integration with commit flow

### Success Criteria
- [ ] overlaps() correctly identifies ancestor/descendant/equal paths
- [ ] Read at path X conflicts with write at overlapping path Y
- [ ] Write at path X conflicts with write at overlapping path Y
- [ ] Conflict detection integrated into existing validation pipeline
- [ ] Version mismatch (stale read) detected and reported
- [ ] Conflicts abort entire transaction (including other primitives)

### Component Breakdown
- **Story #249 (GitHub #287)**: Path Overlap Detection
- **Story #250 (GitHub #288)**: Read-Write Conflict Check
- **Story #251 (GitHub #289)**: Write-Write Conflict Check
- **Story #252 (GitHub #290)**: Conflict Integration with Commit

---

## Story #287: Path Overlap Detection

**GitHub Issue**: [#287](https://github.com/anibjoshi/in-mem/issues/287)
**Estimated Time**: 2 hours
**Dependencies**: Epic 30 complete

### Start Story

```bash
gh issue view 287
./scripts/start-story.sh 31 287 path-overlap
```

### Implementation

This was implemented in Epic 26 Story #227. This story adds additional helper methods.

Add to `crates/core/src/json_types.rs`:

```rust
impl JsonPath {
    /// Check if this path is a strict ancestor (not equal)
    pub fn is_strict_ancestor_of(&self, other: &JsonPath) -> bool {
        self.segments.len() < other.segments.len() && self.is_ancestor_of(other)
    }

    /// Check if this path is a strict descendant (not equal)
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
}
```

### Tests

```rust
#[test]
fn test_path_strict_ancestor() {
    let p1 = JsonPath::parse("foo").unwrap();
    let p2 = JsonPath::parse("foo.bar").unwrap();

    assert!(p1.is_strict_ancestor_of(&p2));
    assert!(!p2.is_strict_ancestor_of(&p1));
    assert!(!p1.is_strict_ancestor_of(&p1)); // Not strict if equal
}

#[test]
fn test_path_common_ancestor() {
    let p1 = JsonPath::parse("foo.bar.baz").unwrap();
    let p2 = JsonPath::parse("foo.bar.qux").unwrap();

    let common = p1.common_ancestor(&p2);
    assert_eq!(common.to_string(), "$.foo.bar");
}
```

### Complete Story

```bash
./scripts/complete-story.sh 287
```

---

## Story #288: Read-Write Conflict Check

**GitHub Issue**: [#288](https://github.com/anibjoshi/in-mem/issues/288)
**Estimated Time**: 3 hours
**Dependencies**: Story #287

### Start Story

```bash
gh issue view 288
./scripts/start-story.sh 31 288 read-write-conflict
```

### Implementation

Create `crates/concurrency/src/json_conflict.rs`:

```rust
//! JSON conflict detection

use in_mem_core::{Key, JsonPath};
use crate::transaction::{JsonPathRead, JsonPatchEntry};

/// Result of conflict detection
#[derive(Debug, Clone)]
pub enum ConflictResult {
    NoConflict,
    ReadWriteConflict {
        key: Key,
        read_path: JsonPath,
        write_path: JsonPath,
    },
    WriteWriteConflict {
        key: Key,
        path1: JsonPath,
        path2: JsonPath,
    },
    VersionMismatch {
        key: Key,
        expected: u64,
        found: u64,
    },
}

/// Check for read-write conflicts
pub fn check_read_write_conflicts(
    reads: &[JsonPathRead],
    writes: &[JsonPatchEntry],
) -> Vec<ConflictResult> {
    let mut conflicts = Vec::new();

    for read in reads {
        for write in writes {
            if read.key != write.key {
                continue;
            }

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

/// Find first read-write conflict (fast failure)
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
```

### Tests

```rust
#[test]
fn test_read_write_conflict_detection() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let reads = vec![
        JsonPathRead::new(key.clone(), JsonPath::parse("foo.bar").unwrap(), 1),
    ];

    let writes = vec![
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
```

### Complete Story

```bash
./scripts/complete-story.sh 288
```

---

## Story #289: Write-Write Conflict Check

**GitHub Issue**: [#289](https://github.com/anibjoshi/in-mem/issues/289)
**Estimated Time**: 2 hours
**Dependencies**: Story #288

### Start Story

```bash
gh issue view 289
./scripts/start-story.sh 31 289 write-write-conflict
```

### Implementation

Add to `crates/concurrency/src/json_conflict.rs`:

```rust
/// Check for write-write conflicts
pub fn check_write_write_conflicts(
    writes: &[JsonPatchEntry],
) -> Vec<ConflictResult> {
    let mut conflicts = Vec::new();

    for (i, w1) in writes.iter().enumerate() {
        for w2 in writes.iter().skip(i + 1) {
            if w1.key != w2.key {
                continue;
            }

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

/// Check for version conflicts (stale reads)
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
    // 1. Version conflicts first (fastest)
    for conflict in check_version_conflicts(snapshot_versions, current_versions) {
        if let ConflictResult::VersionMismatch { key, expected, found } = conflict {
            return Err(JsonConflictError::VersionMismatch { key, expected, found });
        }
    }

    // 2. Write-write conflicts
    if let Some(conflict) = find_first_write_write_conflict(writes) {
        return Err(conflict.into());
    }

    // 3. Read-write conflicts
    if let Some(conflict) = find_first_read_write_conflict(reads, writes) {
        return Err(conflict.into());
    }

    Ok(())
}
```

### Tests

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
}

#[test]
fn test_no_write_conflict_disjoint_paths() {
    let key = Key::new_json(Namespace::for_run(RunId::new()), &JsonDocId::new());

    let writes = vec![
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("foo").unwrap(), 1), 2),
        JsonPatchEntry::new(key.clone(), JsonPatch::set(JsonPath::parse("bar").unwrap(), 2), 3),
    ];

    let conflicts = check_write_write_conflicts(&writes);
    assert!(conflicts.is_empty());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 289
```

---

## Story #290: Conflict Integration with Commit

**GitHub Issue**: [#290](https://github.com/anibjoshi/in-mem/issues/290)
**Estimated Time**: 3 hours
**Dependencies**: Story #289

### Start Story

```bash
gh issue view 290
./scripts/start-story.sh 31 290 conflict-commit
```

### Implementation

Update `crates/engine/src/database.rs`:

```rust
use in_mem_concurrency::json_conflict::{check_all_conflicts, JsonConflictError};

impl Database {
    /// Validate all JSON conflicts before commit
    pub fn validate_json_transaction(
        &self,
        ctx: &TransactionContext,
    ) -> Result<(), TransactionError> {
        if !ctx.has_json_ops() {
            return Ok(());
        }

        // Get current versions
        let current_versions = self.get_current_json_versions(ctx)?;

        let reads = ctx.json_reads().map(|v| v.as_slice()).unwrap_or(&[]);
        let writes = ctx.json_writes().map(|v| v.as_slice()).unwrap_or(&[]);
        let snapshot = ctx.json_snapshot_versions().cloned().unwrap_or_default();

        check_all_conflicts(reads, writes, &snapshot, &current_versions)
            .map_err(TransactionError::JsonConflict)?;

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    // ... existing variants ...

    #[error("JSON conflict: {0}")]
    JsonConflict(#[from] JsonConflictError),
}
```

### Tests

```rust
#[test]
fn test_commit_detects_write_write_conflict() {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    let json = JsonStore::new(db.clone());
    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    let result = db.transaction(run_id, |txn| {
        // Two overlapping writes
        txn.json_set(&key, &JsonPath::parse("foo").unwrap(), JsonValue::from(1))?;
        txn.json_set(&key, &JsonPath::parse("foo.bar").unwrap(), JsonValue::from(2))?;
        Ok(())
    });

    assert!(matches!(result, Err(TransactionError::JsonConflict(
        JsonConflictError::WriteWriteConflict { .. }
    ))));
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

### Complete Story

```bash
./scripts/complete-story.sh 290
```

---

## Epic 31 Completion Checklist

### Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-concurrency -- json_conflict
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
```

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-31-conflict-detection -m "Epic 31: Conflict Detection complete

Delivered:
- Path overlap detection enhancements
- Read-write conflict check
- Write-write conflict check
- Version mismatch detection
- Conflict integration with commit flow

Stories: #287, #288, #289, #290
"
git push origin develop
gh issue close 261 --comment "Epic 31: Conflict Detection - COMPLETE"
```
