# Epic 18: RunIndex Primitive - Implementation Prompts

**Epic Goal**: First-class run lifecycle management.

**GitHub Issue**: [#164](https://github.com/anibjoshi/in-mem/issues/164)
**Status**: Ready to begin (after Epic 13)
**Dependencies**: Epic 13 (Primitives Foundation) complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M3_ARCHITECTURE.md` is the GOSPEL for ALL M3 implementation.**

Before starting ANY story in this epic, read:
- Section 8: RunIndex Primitive
- Section 8.3: Status Transitions (CRITICAL)
- Section 12.2: RunIndex Status Transitions

See `docs/prompts/M3_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 18 Overview

### Critical Design Decision: Status Transition Validation

RunIndex ENFORCES valid status transitions. Invalid transitions return an error.

```
                    create_run()
                         |
                         v
                    +---------+
            +------>| Active  |<------+
            |       +---------+       |
            |       /    |    \       |
            |      /     |     \      |
            | complete() |  fail()    |
            |    /       |       \    |
            |   v        |        v   |
            | +----------+   +---------+
    resume()|>| Completed|   | Failed  |
            | +----+-----+   +----+----+
            |      |              |
            |      | archive()    | archive()
            |      v              v
            | +----------+   +---------+
            | | Archived |   | Archived|
            | +----------+   +---------+
```

**VALID TRANSITIONS:**
- Active -> Completed, Failed, Cancelled, Paused, Archived
- Paused -> Active, Cancelled, Archived
- Completed -> Archived
- Failed -> Archived
- Cancelled -> Archived

**INVALID (will error):**
- Completed -> Active (no resurrection)
- Failed -> Active (no resurrection)
- Archived -> * (terminal)
- Failed -> Completed (no retroactive fix)

### Scope
- RunIndex struct as stateless facade
- RunMetadata and RunStatus structures
- Run lifecycle: create, get, update_status
- Status transition validation
- Query operations with filters
- Cascading delete and soft archive
- Secondary indices: by-status, by-tag, by-parent

### Success Criteria
- [ ] RunIndex struct implemented with `Arc<Database>` reference
- [ ] RunStatus enum with all states
- [ ] RunMetadata struct with all fields
- [ ] `create_run()` creates with Active status
- [ ] `update_status()` validates transitions
- [ ] `delete_run()` performs cascading hard delete
- [ ] `archive_run()` performs soft delete
- [ ] Status transition validation enforced
- [ ] All unit tests pass (>95% coverage)

### Component Breakdown
- **Story #191**: RunIndex Core & RunMetadata Structures - BLOCKS others
- **Story #192**: RunIndex Create & Get Operations
- **Story #193**: RunIndex Status Update & Transition Validation
- **Story #194**: RunIndex Query Operations & Indices
- **Story #195**: RunIndex Delete & Archive Operations
- **Story #196**: RunIndex Integration with Other Primitives

---

## Dependency Graph

```
Phase 1 (Sequential):
  Story #191 (RunIndex Core)
    └─> BLOCKS #192, #193

Phase 2 (Parallel - 3 Claudes after #191):
  Story #192 (Create & Get)
  Story #193 (Status Transitions)
  Story #194 (Query Operations)
    └─> All depend on #191
    └─> Independent of each other

Phase 3 (Sequential):
  Story #195 (Delete & Archive)
    └─> Depends on #191-#194

Phase 4 (Sequential):
  Story #196 (Integration)
    └─> Depends on ALL previous stories
```

---

## Story #191: RunIndex Core & RunMetadata Structures

**GitHub Issue**: [#191](https://github.com/anibjoshi/in-mem/issues/191)
**Estimated Time**: 4 hours
**Dependencies**: Epic 13 complete
**Blocks**: Stories #192, #193, #194

### Start Story

```bash
/opt/homebrew/bin/gh issue view 191
./scripts/start-story.sh 18 191 runindex-core
```

### Implementation

Create `crates/primitives/src/run_index.rs`:

```rust
//! RunIndex: First-class run lifecycle management
//!
//! ## Status Transitions
//!
//! RunIndex enforces valid status transitions. Invalid transitions
//! (like resurrection from Completed/Failed to Active) return errors.
//!
//! ## Cascading Delete
//!
//! `delete_run()` removes ALL data for a run across ALL primitives
//! (KV, Events, States, Traces).

use std::sync::Arc;
use serde::{Serialize, Deserialize};
use in_mem_engine::Database;
use in_mem_core::{Key, Namespace, RunId, Value, Result, Error};

/// Run lifecycle status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RunStatus {
    /// Run is currently executing
    Active,
    /// Run completed successfully
    Completed,
    /// Run failed with error
    Failed,
    /// Run was cancelled
    Cancelled,
    /// Run is paused (can resume)
    Paused,
    /// Run is archived (terminal, soft delete)
    Archived,
}

impl RunStatus {
    /// Check if this is a terminal status
    pub fn is_terminal(&self) -> bool {
        matches!(self, RunStatus::Archived)
    }

    /// Check if transition from current to target is valid
    pub fn can_transition_to(&self, target: RunStatus) -> bool {
        match (self, target) {
            // From Active: can go anywhere
            (RunStatus::Active, _) => true,

            // From Paused: can resume, cancel, or archive
            (RunStatus::Paused, RunStatus::Active) => true,
            (RunStatus::Paused, RunStatus::Cancelled) => true,
            (RunStatus::Paused, RunStatus::Archived) => true,

            // From terminal states: can only archive
            (RunStatus::Completed, RunStatus::Archived) => true,
            (RunStatus::Failed, RunStatus::Archived) => true,
            (RunStatus::Cancelled, RunStatus::Archived) => true,

            // Archived is terminal - no transitions allowed
            (RunStatus::Archived, _) => false,

            // All other transitions are invalid (no resurrection)
            _ => false,
        }
    }
}

/// Metadata about a run
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunMetadata {
    /// Unique run identifier
    pub run_id: String,
    /// Parent run if forked
    pub parent_run: Option<String>,
    /// Current status
    pub status: RunStatus,
    /// Creation timestamp
    pub created_at: i64,
    /// Last update timestamp
    pub updated_at: i64,
    /// Completion timestamp (if finished)
    pub completed_at: Option<i64>,
    /// User-defined tags
    pub tags: Vec<String>,
    /// Custom metadata
    pub metadata: Value,
    /// Error message if failed
    pub error: Option<String>,
}

impl RunMetadata {
    /// Create new run metadata
    pub fn new(run_id: &str) -> Self {
        let now = Self::now();
        Self {
            run_id: run_id.to_string(),
            parent_run: None,
            status: RunStatus::Active,
            created_at: now,
            updated_at: now,
            completed_at: None,
            tags: vec![],
            metadata: Value::Null,
            error: None,
        }
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }
}

/// Run lifecycle management primitive
#[derive(Clone)]
pub struct RunIndex {
    db: Arc<Database>,
}

impl RunIndex {
    /// Create new RunIndex instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build key for run metadata
    fn key_for(&self, run_id: &str) -> Key {
        // RunIndex uses a global namespace, not run-scoped
        Key::new_run(Namespace::global(), run_id)
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_transitions() {
        // Active can go anywhere
        assert!(RunStatus::Active.can_transition_to(RunStatus::Completed));
        assert!(RunStatus::Active.can_transition_to(RunStatus::Failed));
        assert!(RunStatus::Active.can_transition_to(RunStatus::Paused));
        assert!(RunStatus::Active.can_transition_to(RunStatus::Archived));

        // Paused can resume or archive
        assert!(RunStatus::Paused.can_transition_to(RunStatus::Active));
        assert!(RunStatus::Paused.can_transition_to(RunStatus::Archived));

        // No resurrection
        assert!(!RunStatus::Completed.can_transition_to(RunStatus::Active));
        assert!(!RunStatus::Failed.can_transition_to(RunStatus::Active));

        // Can only archive from terminal states
        assert!(RunStatus::Completed.can_transition_to(RunStatus::Archived));
        assert!(RunStatus::Failed.can_transition_to(RunStatus::Archived));

        // Archived is terminal
        assert!(!RunStatus::Archived.can_transition_to(RunStatus::Active));
        assert!(!RunStatus::Archived.can_transition_to(RunStatus::Completed));
    }

    #[test]
    fn test_run_metadata_creation() {
        let meta = RunMetadata::new("run-123");
        assert_eq!(meta.status, RunStatus::Active);
        assert!(meta.created_at > 0);
        assert!(meta.parent_run.is_none());
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 191
```

---

## Story #192: RunIndex Create & Get Operations

**GitHub Issue**: [#192](https://github.com/anibjoshi/in-mem/issues/192)
**Estimated Time**: 4 hours
**Dependencies**: Story #191

### Implementation

```rust
impl RunIndex {
    /// Create a new run
    pub fn create_run(&self, run_id: &str) -> Result<RunMetadata> {
        self.create_run_with_options(run_id, None, vec![], Value::Null)
    }

    /// Create a new run with options
    pub fn create_run_with_options(
        &self,
        run_id: &str,
        parent_run: Option<String>,
        tags: Vec<String>,
        metadata: Value,
    ) -> Result<RunMetadata> {
        self.db.transaction_global(|txn| {
            let key = self.key_for(run_id);

            // Check if run already exists
            if txn.get(&key)?.is_some() {
                return Err(Error::AlreadyExists(format!("Run '{}' already exists", run_id)));
            }

            // Validate parent exists if specified
            if let Some(ref parent_id) = parent_run {
                let parent_key = self.key_for(parent_id);
                if txn.get(&parent_key)?.is_none() {
                    return Err(Error::NotFound(format!("Parent run '{}' not found", parent_id)));
                }
            }

            let mut run_meta = RunMetadata::new(run_id);
            run_meta.parent_run = parent_run.clone();
            run_meta.tags = tags;
            run_meta.metadata = metadata;

            txn.put(key, Value::from_json(serde_json::to_value(&run_meta)?)?)?;

            // Write indices
            self.write_indices(txn, &run_meta)?;

            Ok(run_meta)
        })
    }

    /// Get run metadata
    pub fn get_run(&self, run_id: &str) -> Result<Option<RunMetadata>> {
        self.db.transaction_global(|txn| {
            let key = self.key_for(run_id);
            match txn.get(&key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }

    /// List all run IDs
    pub fn list_runs(&self) -> Result<Vec<String>> {
        self.db.transaction_global(|txn| {
            let prefix = Key::new_run(Namespace::global(), "");
            let results = txn.scan_prefix(&prefix)?;
            Ok(results
                .into_iter()
                .filter_map(|(k, _)| k.user_key_string())
                .collect())
        })
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 192
```

---

## Story #193: RunIndex Status Update & Transition Validation

**GitHub Issue**: [#193](https://github.com/anibjoshi/in-mem/issues/193)
**Estimated Time**: 5 hours
**Dependencies**: Story #191

### Implementation

```rust
impl RunIndex {
    /// Update run status with transition validation
    pub fn update_status(&self, run_id: &str, new_status: RunStatus) -> Result<RunMetadata> {
        self.db.transaction_global(|txn| {
            let key = self.key_for(run_id);

            let mut run_meta: RunMetadata = match txn.get(&key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Err(Error::NotFound(format!("Run '{}' not found", run_id))),
            };

            // Validate transition
            if !run_meta.status.can_transition_to(new_status) {
                return Err(Error::InvalidTransition {
                    from: format!("{:?}", run_meta.status),
                    to: format!("{:?}", new_status),
                });
            }

            let old_status = run_meta.status;
            run_meta.status = new_status;
            run_meta.updated_at = RunMetadata::now();

            // Set completed_at for terminal-ish states
            if matches!(new_status, RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled) {
                run_meta.completed_at = Some(run_meta.updated_at);
            }

            txn.put(key, Value::from_json(serde_json::to_value(&run_meta)?)?)?;

            // Update status index
            self.update_status_index(txn, run_id, old_status, new_status)?;

            Ok(run_meta)
        })
    }

    /// Complete a run successfully
    pub fn complete_run(&self, run_id: &str) -> Result<RunMetadata> {
        self.update_status(run_id, RunStatus::Completed)
    }

    /// Fail a run with error
    pub fn fail_run(&self, run_id: &str, error: &str) -> Result<RunMetadata> {
        self.db.transaction_global(|txn| {
            let key = self.key_for(run_id);

            let mut run_meta: RunMetadata = match txn.get(&key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Err(Error::NotFound(format!("Run '{}' not found", run_id))),
            };

            if !run_meta.status.can_transition_to(RunStatus::Failed) {
                return Err(Error::InvalidTransition {
                    from: format!("{:?}", run_meta.status),
                    to: "Failed".to_string(),
                });
            }

            let old_status = run_meta.status;
            run_meta.status = RunStatus::Failed;
            run_meta.error = Some(error.to_string());
            run_meta.updated_at = RunMetadata::now();
            run_meta.completed_at = Some(run_meta.updated_at);

            txn.put(key, Value::from_json(serde_json::to_value(&run_meta)?)?)?;
            self.update_status_index(txn, run_id, old_status, RunStatus::Failed)?;

            Ok(run_meta)
        })
    }

    /// Pause a run
    pub fn pause_run(&self, run_id: &str) -> Result<RunMetadata> {
        self.update_status(run_id, RunStatus::Paused)
    }

    /// Resume a paused run
    pub fn resume_run(&self, run_id: &str) -> Result<RunMetadata> {
        self.update_status(run_id, RunStatus::Active)
    }

    /// Cancel a run
    pub fn cancel_run(&self, run_id: &str) -> Result<RunMetadata> {
        self.update_status(run_id, RunStatus::Cancelled)
    }
}
```

### Tests

```rust
#[test]
fn test_valid_transitions() {
    let (_temp, db, ri) = setup();

    let meta = ri.create_run("run-1").unwrap();
    assert_eq!(meta.status, RunStatus::Active);

    let meta = ri.complete_run("run-1").unwrap();
    assert_eq!(meta.status, RunStatus::Completed);
    assert!(meta.completed_at.is_some());
}

#[test]
fn test_invalid_resurrection() {
    let (_temp, db, ri) = setup();

    ri.create_run("run-1").unwrap();
    ri.complete_run("run-1").unwrap();

    // Cannot go from Completed to Active
    let result = ri.update_status("run-1", RunStatus::Active);
    assert!(matches!(result, Err(Error::InvalidTransition { .. })));
}

#[test]
fn test_fail_with_error() {
    let (_temp, db, ri) = setup();

    ri.create_run("run-1").unwrap();
    let meta = ri.fail_run("run-1", "Something went wrong").unwrap();

    assert_eq!(meta.status, RunStatus::Failed);
    assert_eq!(meta.error, Some("Something went wrong".to_string()));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 193
```

---

## Story #194: RunIndex Query Operations & Indices

**GitHub Issue**: [#194](https://github.com/anibjoshi/in-mem/issues/194)
**Estimated Time**: 4 hours
**Dependencies**: Story #191

### Implementation

```rust
impl RunIndex {
    /// Write secondary indices for a run
    fn write_indices(&self, txn: &mut TransactionContext, meta: &RunMetadata) -> Result<()> {
        // Index by status
        let status_key = Key::new_run_index(
            Namespace::global(),
            "by-status",
            &format!("{:?}", meta.status),
            &meta.run_id,
        );
        txn.put(status_key, Value::String(meta.run_id.clone()))?;

        // Index by each tag
        for tag in &meta.tags {
            let tag_key = Key::new_run_index(
                Namespace::global(),
                "by-tag",
                tag,
                &meta.run_id,
            );
            txn.put(tag_key, Value::String(meta.run_id.clone()))?;
        }

        // Index by parent
        if let Some(ref parent) = meta.parent_run {
            let parent_key = Key::new_run_index(
                Namespace::global(),
                "by-parent",
                parent,
                &meta.run_id,
            );
            txn.put(parent_key, Value::String(meta.run_id.clone()))?;
        }

        Ok(())
    }

    /// Update status index on transition
    fn update_status_index(
        &self,
        txn: &mut TransactionContext,
        run_id: &str,
        old_status: RunStatus,
        new_status: RunStatus,
    ) -> Result<()> {
        // Remove old status index
        let old_key = Key::new_run_index(
            Namespace::global(),
            "by-status",
            &format!("{:?}", old_status),
            run_id,
        );
        txn.delete(&old_key)?;

        // Add new status index
        let new_key = Key::new_run_index(
            Namespace::global(),
            "by-status",
            &format!("{:?}", new_status),
            run_id,
        );
        txn.put(new_key, Value::String(run_id.to_string()))?;

        Ok(())
    }

    /// Query runs by status
    pub fn query_by_status(&self, status: RunStatus) -> Result<Vec<RunMetadata>> {
        self.db.transaction_global(|txn| {
            let prefix = Key::new_run_index(
                Namespace::global(),
                "by-status",
                &format!("{:?}", status),
                "",
            );

            let results = txn.scan_prefix(&prefix)?;
            let run_ids: Vec<String> = results
                .into_iter()
                .filter_map(|(_, v)| v.as_string().map(|s| s.to_string()))
                .collect();

            let mut runs = Vec::new();
            for id in run_ids {
                if let Some(meta) = self.get_run(&id)? {
                    runs.push(meta);
                }
            }
            Ok(runs)
        })
    }

    /// Query runs by tag
    pub fn query_by_tag(&self, tag: &str) -> Result<Vec<RunMetadata>> {
        self.db.transaction_global(|txn| {
            let prefix = Key::new_run_index(Namespace::global(), "by-tag", tag, "");

            let results = txn.scan_prefix(&prefix)?;
            let run_ids: Vec<String> = results
                .into_iter()
                .filter_map(|(_, v)| v.as_string().map(|s| s.to_string()))
                .collect();

            let mut runs = Vec::new();
            for id in run_ids {
                if let Some(meta) = self.get_run(&id)? {
                    runs.push(meta);
                }
            }
            Ok(runs)
        })
    }

    /// Get child runs
    pub fn get_child_runs(&self, parent_id: &str) -> Result<Vec<RunMetadata>> {
        self.db.transaction_global(|txn| {
            let prefix = Key::new_run_index(Namespace::global(), "by-parent", parent_id, "");

            let results = txn.scan_prefix(&prefix)?;
            let run_ids: Vec<String> = results
                .into_iter()
                .filter_map(|(_, v)| v.as_string().map(|s| s.to_string()))
                .collect();

            let mut runs = Vec::new();
            for id in run_ids {
                if let Some(meta) = self.get_run(&id)? {
                    runs.push(meta);
                }
            }
            Ok(runs)
        })
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 194
```

---

## Story #195: RunIndex Delete & Archive Operations

**GitHub Issue**: [#195](https://github.com/anibjoshi/in-mem/issues/195)
**Estimated Time**: 5 hours
**Dependencies**: Stories #191-#194

### Implementation

```rust
impl RunIndex {
    /// Archive a run (soft delete - status change to Archived)
    pub fn archive_run(&self, run_id: &str) -> Result<RunMetadata> {
        self.update_status(run_id, RunStatus::Archived)
    }

    /// Delete a run (HARD DELETE - removes ALL data for this run)
    ///
    /// This is a cascading delete that removes:
    /// - Run metadata
    /// - All KV data for the run
    /// - All Events for the run
    /// - All StateCell data for the run
    /// - All Traces for the run
    /// - All secondary indices
    ///
    /// USE WITH CAUTION - this is irreversible!
    pub fn delete_run(&self, run_id: &str) -> Result<()> {
        // First verify the run exists
        let run_meta = self.get_run(run_id)?
            .ok_or_else(|| Error::NotFound(format!("Run '{}' not found", run_id)))?;

        // Delete in a global transaction
        self.db.transaction_global(|txn| {
            // Delete run metadata
            let meta_key = self.key_for(run_id);
            txn.delete(&meta_key)?;

            // Delete status index
            let status_key = Key::new_run_index(
                Namespace::global(),
                "by-status",
                &format!("{:?}", run_meta.status),
                run_id,
            );
            txn.delete(&status_key)?;

            // Delete tag indices
            for tag in &run_meta.tags {
                let tag_key = Key::new_run_index(
                    Namespace::global(),
                    "by-tag",
                    tag,
                    run_id,
                );
                txn.delete(&tag_key)?;
            }

            // Delete parent index
            if let Some(ref parent) = run_meta.parent_run {
                let parent_key = Key::new_run_index(
                    Namespace::global(),
                    "by-parent",
                    parent,
                    run_id,
                );
                txn.delete(&parent_key)?;
            }

            Ok(())
        })?;

        // Delete all run-scoped data (this requires a run-scoped transaction)
        let run_id_typed = RunId::from_string(run_id);
        self.db.transaction(&run_id_typed, |txn| {
            // Delete all KV data
            let kv_prefix = Key::new_kv(Namespace::for_run(&run_id_typed), "");
            self.delete_by_prefix(txn, &kv_prefix)?;

            // Delete all Events
            let event_prefix = Key::new_event(Namespace::for_run(&run_id_typed), 0);
            self.delete_by_prefix(txn, &event_prefix)?;

            // Delete event metadata
            let event_meta = Key::new_event_meta(Namespace::for_run(&run_id_typed));
            txn.delete(&event_meta)?;

            // Delete all StateCells
            let state_prefix = Key::new_state(Namespace::for_run(&run_id_typed), "");
            self.delete_by_prefix(txn, &state_prefix)?;

            // Delete all Traces
            let trace_prefix = Key::new_trace(Namespace::for_run(&run_id_typed), "");
            self.delete_by_prefix(txn, &trace_prefix)?;

            Ok(())
        })
    }

    /// Delete all keys with a given prefix
    fn delete_by_prefix(&self, txn: &mut TransactionContext, prefix: &Key) -> Result<()> {
        let results = txn.scan_prefix(prefix)?;
        for (key, _) in results {
            txn.delete(&key)?;
        }
        Ok(())
    }

    /// Add tags to a run
    pub fn add_tags(&self, run_id: &str, tags: Vec<String>) -> Result<RunMetadata> {
        self.db.transaction_global(|txn| {
            let key = self.key_for(run_id);

            let mut meta: RunMetadata = match txn.get(&key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Err(Error::NotFound(format!("Run '{}' not found", run_id))),
            };

            // Add new tags
            for tag in &tags {
                if !meta.tags.contains(tag) {
                    meta.tags.push(tag.clone());

                    // Add tag index
                    let tag_key = Key::new_run_index(
                        Namespace::global(),
                        "by-tag",
                        tag,
                        run_id,
                    );
                    txn.put(tag_key, Value::String(run_id.to_string()))?;
                }
            }

            meta.updated_at = RunMetadata::now();
            txn.put(key, Value::from_json(serde_json::to_value(&meta)?)?)?;

            Ok(meta)
        })
    }

    /// Update custom metadata
    pub fn update_metadata(&self, run_id: &str, metadata: Value) -> Result<RunMetadata> {
        self.db.transaction_global(|txn| {
            let key = self.key_for(run_id);

            let mut meta: RunMetadata = match txn.get(&key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Err(Error::NotFound(format!("Run '{}' not found", run_id))),
            };

            meta.metadata = metadata;
            meta.updated_at = RunMetadata::now();
            txn.put(key, Value::from_json(serde_json::to_value(&meta)?)?)?;

            Ok(meta)
        })
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 195
```

---

## Story #196: RunIndex Integration with Other Primitives

**GitHub Issue**: [#196](https://github.com/anibjoshi/in-mem/issues/196)
**Estimated Time**: 4 hours
**Dependencies**: Stories #191-#195

### Implementation

Update `crates/primitives/src/lib.rs`:

```rust
pub mod run_index;
pub use run_index::{RunIndex, RunMetadata, RunStatus};
```

Add integration tests that verify:
1. Creating a run through RunIndex
2. Using that run_id with KVStore, EventLog, StateCell, TraceStore
3. Deleting the run cascades to all primitives

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{KVStore, EventLog, StateCell, TraceStore};

    #[test]
    fn test_run_lifecycle_with_primitives() {
        let temp = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp.path()).unwrap());

        let run_index = RunIndex::new(db.clone());
        let kv = KVStore::new(db.clone());
        let event_log = EventLog::new(db.clone());
        let state_cell = StateCell::new(db.clone());
        let trace_store = TraceStore::new(db.clone());

        // Create run
        let meta = run_index.create_run("test-run").unwrap();
        let run_id = RunId::from_string(&meta.run_id);

        // Use primitives
        kv.put(&run_id, "key", Value::I64(42)).unwrap();
        event_log.append(&run_id, "test", Value::Null).unwrap();
        state_cell.init(&run_id, "cell", Value::Bool(true)).unwrap();
        trace_store.record(&run_id, TraceType::Thought {
            content: "test".into(),
            confidence: None,
        }, vec![], Value::Null).unwrap();

        // Verify data exists
        assert!(kv.get(&run_id, "key").unwrap().is_some());
        assert_eq!(event_log.len(&run_id).unwrap(), 1);
        assert!(state_cell.exists(&run_id, "cell").unwrap());

        // Delete run (cascading)
        run_index.delete_run("test-run").unwrap();

        // Verify all data is gone
        assert!(kv.get(&run_id, "key").unwrap().is_none());
        assert_eq!(event_log.len(&run_id).unwrap(), 0);
        assert!(!state_cell.exists(&run_id, "cell").unwrap());
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 196
```

---

## Epic 18 Completion Checklist

### Verify Deliverables

- [ ] RunIndex struct is stateless
- [ ] RunStatus enum has all states
- [ ] RunMetadata has all fields
- [ ] Status transitions validated
- [ ] No resurrection allowed
- [ ] Archived is terminal
- [ ] Cascading delete works
- [ ] Secondary indices work
- [ ] Integration with other primitives verified
- [ ] All tests pass

### Merge and Close

```bash
git checkout develop
git merge --no-ff epic-18-runindex-primitive -m "Epic 18: RunIndex Primitive

Complete:
- RunIndex stateless facade
- RunStatus enum (Active, Completed, Failed, Cancelled, Paused, Archived)
- RunMetadata structure
- Status transition validation (no resurrection, archived is terminal)
- Cascading delete (removes all data for run)
- Archive (soft delete)
- Secondary indices (by-status, by-tag, by-parent)
- Integration with all other primitives

Stories: #191, #192, #193, #194, #195, #196
"

/opt/homebrew/bin/gh issue close 164 --comment "Epic 18: RunIndex Primitive - COMPLETE"
```

---

## Summary

Epic 18 implements the RunIndex primitive - run lifecycle management. Key design decisions:
- Status transitions are ENFORCED (no resurrection)
- Archived is terminal state
- Cascading delete removes ALL data across ALL primitives
