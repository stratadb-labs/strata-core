# Epic 43: Run Lifecycle & Replay - Implementation Prompts

**Epic Goal**: Implement run lifecycle and deterministic replay

**GitHub Issue**: [#341](https://github.com/anibjoshi/in-mem/issues/341)
**Status**: Ready to begin (after Epic 41, 42 complete)
**Dependencies**: Epic 41 (Crash Recovery), Epic 42 (WAL Enhancement)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M7_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M7_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M7/EPIC_43_RUN_REPLAY.md`
3. **Prompt Header**: `docs/prompts/M7/M7_PROMPT_HEADER.md` for the 5 architectural rules

---

## Epic 43 Overview

### Scope
- RunStatus enum and RunMetadata type
- begin_run() and end_run() implementations
- RunIndex with event offset tracking
- replay_run() returning ReadOnlyView
- diff_runs() for key-level comparison
- Orphaned run detection

### Key Rules: Replay Invariants (P1-P6)

| # | Invariant | Meaning |
|---|-----------|---------|
| P1 | Pure function | Over (Snapshot, WAL, EventLog) |
| P2 | Side-effect free | Does not mutate canonical store |
| P3 | Derived view | Not a new source of truth |
| P4 | Does not persist | Unless explicitly materialized |
| P5 | Deterministic | Same inputs = Same view |
| P6 | Idempotent | Running twice produces identical view |

**CRITICAL**: Replay NEVER writes to the canonical store. The ReadOnlyView is derived, not authoritative.

### Success Criteria
- [ ] RunStatus: Active, Completed, Orphaned, NotFound
- [ ] begin_run() writes WAL entry, creates run metadata
- [ ] end_run() writes WAL entry, marks run completed
- [ ] RunIndex tracks event offsets for O(run size) replay
- [ ] replay_run() returns ReadOnlyView (doesn't mutate canonical store)
- [ ] diff_runs() compares two runs at key level
- [ ] Orphaned runs (no end marker) detected

### Component Breakdown
- **Story #310 (GitHub #365)**: RunStatus Enum and RunMetadata Type - FOUNDATION
- **Story #311 (GitHub #366)**: begin_run() Implementation - CRITICAL
- **Story #312 (GitHub #367)**: end_run() Implementation - CRITICAL
- **Story #313 (GitHub #368)**: RunIndex Event Offset Tracking - CRITICAL
- **Story #314 (GitHub #369)**: replay_run() -> ReadOnlyView - CRITICAL
- **Story #315 (GitHub #370)**: diff_runs() Key-Level Comparison - HIGH
- **Story #316 (GitHub #371)**: Orphaned Run Detection - HIGH

---

## Dependency Graph

```
Story #365 (Types) ──> Story #366 (begin_run) ──> Story #367 (end_run)
        │                      │
        v                      v
Story #368 (RunIndex) ────> Story #369 (replay_run)
                                    │
                                    v
                            Story #370 (diff_runs)
                                    │
                                    v
                            Story #371 (Orphaned)
```

---

## Story #365: RunStatus Enum and RunMetadata Type

**GitHub Issue**: [#365](https://github.com/anibjoshi/in-mem/issues/365)
**Estimated Time**: 2 hours
**Dependencies**: None
**Blocks**: All other stories

### Start Story

```bash
gh issue view 365
./scripts/start-story.sh 43 365 run-types
```

### Implementation

Create `crates/core/src/run_types.rs`:

```rust
//! Run lifecycle types

use crate::types::RunId;

/// Status of a run
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    /// Run is active (begin_run called, end_run not yet called)
    Active,
    /// Run completed normally (end_run called)
    Completed,
    /// Run was never ended (orphaned - no end_run marker)
    Orphaned,
    /// Run doesn't exist
    NotFound,
}

/// Metadata for a run
#[derive(Debug, Clone)]
pub struct RunMetadata {
    /// Run ID
    pub run_id: RunId,
    /// Current status
    pub status: RunStatus,
    /// When run started (microseconds since epoch)
    pub started_at: u64,
    /// When run ended (if completed)
    pub ended_at: Option<u64>,
    /// Number of events in this run
    pub event_count: u64,
}

impl RunMetadata {
    /// Create metadata for a new run
    pub fn new(run_id: RunId, started_at: u64) -> Self {
        RunMetadata {
            run_id,
            status: RunStatus::Active,
            started_at,
            ended_at: None,
            event_count: 0,
        }
    }

    /// Mark run as completed
    pub fn complete(&mut self, ended_at: u64) {
        self.status = RunStatus::Completed;
        self.ended_at = Some(ended_at);
    }

    /// Mark run as orphaned
    pub fn mark_orphaned(&mut self) {
        self.status = RunStatus::Orphaned;
    }

    /// Duration in microseconds (if completed)
    pub fn duration_micros(&self) -> Option<u64> {
        self.ended_at.map(|e| e.saturating_sub(self.started_at))
    }
}
```

### Tests

```rust
#[test]
fn test_run_status_transitions() {
    let mut meta = RunMetadata::new(RunId::new(), 1000);
    assert_eq!(meta.status, RunStatus::Active);

    meta.complete(2000);
    assert_eq!(meta.status, RunStatus::Completed);
    assert_eq!(meta.duration_micros(), Some(1000));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 365
```

---

## Story #366: begin_run() Implementation

**GitHub Issue**: [#366](https://github.com/anibjoshi/in-mem/issues/366)
**Estimated Time**: 3 hours
**Dependencies**: Story #365

### Start Story

```bash
gh issue view 366
./scripts/start-story.sh 43 366 begin-run
```

### Implementation

```rust
impl Database {
    /// Begin a new run
    ///
    /// Creates run metadata and writes WAL entry.
    /// Subsequent operations can be associated with this run_id.
    pub fn begin_run(&self, run_id: RunId) -> Result<(), RunError> {
        // Check run doesn't already exist
        if self.run_index.exists(run_id) {
            return Err(RunError::AlreadyExists(run_id));
        }

        let timestamp = now_micros();

        // Write WAL entry
        let entry = WalEntry {
            entry_type: WalEntryType::RunBegin,
            version: 1,
            tx_id: None, // Run lifecycle is not transactional
            payload: self.serialize_run_begin(run_id, timestamp),
        };
        self.wal.write_entry(&entry)?;

        // Update run index
        let metadata = RunMetadata::new(run_id, timestamp);
        self.run_index.insert(run_id, metadata);

        // Write to EventLog (semantic history)
        self.event_log.append(RunEvent::RunStarted { run_id })?;

        Ok(())
    }

    fn serialize_run_begin(&self, run_id: RunId, timestamp: u64) -> Vec<u8> {
        let mut buf = Vec::with_capacity(24);
        buf.extend_from_slice(run_id.as_bytes());
        buf.extend_from_slice(&timestamp.to_le_bytes());
        buf
    }
}

/// Run errors
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("Run already exists: {0:?}")]
    AlreadyExists(RunId),

    #[error("Run not found: {0:?}")]
    NotFound(RunId),

    #[error("Run not active: {0:?}")]
    NotActive(RunId),

    #[error("WAL error: {0}")]
    Wal(#[from] WalError),
}
```

### Acceptance Criteria

- [ ] Creates RunMetadata with Active status
- [ ] Writes RunBegin WAL entry
- [ ] Returns error if run already exists
- [ ] Writes to EventLog

### Complete Story

```bash
./scripts/complete-story.sh 366
```

---

## Story #367: end_run() Implementation

**GitHub Issue**: [#367](https://github.com/anibjoshi/in-mem/issues/367)
**Estimated Time**: 2 hours
**Dependencies**: Story #366

### Start Story

```bash
gh issue view 367
./scripts/start-story.sh 43 367 end-run
```

### Implementation

```rust
impl Database {
    /// End a run
    ///
    /// Marks run as completed and writes WAL entry.
    pub fn end_run(&self, run_id: RunId) -> Result<(), RunError> {
        // Check run exists and is active
        let meta = self.run_index.get(run_id)?;
        if meta.status != RunStatus::Active {
            return Err(RunError::NotActive(run_id));
        }

        let timestamp = now_micros();

        // Write WAL entry
        let entry = WalEntry {
            entry_type: WalEntryType::RunEnd,
            version: 1,
            tx_id: None,
            payload: self.serialize_run_end(run_id, timestamp),
        };
        self.wal.write_entry(&entry)?;

        // Update run index
        self.run_index.update(run_id, |meta| {
            meta.complete(timestamp);
        })?;

        // Write to EventLog
        self.event_log.append(RunEvent::RunEnded { run_id })?;

        Ok(())
    }

    /// Abort a run (mark as failed)
    pub fn abort_run(&self, run_id: RunId, reason: &str) -> Result<(), RunError> {
        let meta = self.run_index.get(run_id)?;
        if meta.status != RunStatus::Active {
            return Err(RunError::NotActive(run_id));
        }

        let timestamp = now_micros();

        // Write WAL entry (same as end, but metadata indicates failure)
        let entry = WalEntry {
            entry_type: WalEntryType::RunEnd,
            version: 1,
            tx_id: None,
            payload: self.serialize_run_abort(run_id, timestamp, reason),
        };
        self.wal.write_entry(&entry)?;

        self.run_index.update(run_id, |meta| {
            meta.complete(timestamp);
        })?;

        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] Marks run as Completed
- [ ] Writes RunEnd WAL entry
- [ ] Returns error if run not active
- [ ] Writes to EventLog

### Complete Story

```bash
./scripts/complete-story.sh 367
```

---

## Story #368: RunIndex Event Offset Tracking

**GitHub Issue**: [#368](https://github.com/anibjoshi/in-mem/issues/368)
**Estimated Time**: 3 hours
**Dependencies**: Story #365

### Start Story

```bash
gh issue view 368
./scripts/start-story.sh 43 368 run-index
```

### Implementation

```rust
/// Run index maps runs to their events for O(run size) replay
pub struct RunIndex {
    /// Run metadata
    runs: HashMap<RunId, RunMetadata>,
    /// Run -> EventLog offsets
    run_events: HashMap<RunId, Vec<u64>>,
}

impl RunIndex {
    pub fn new() -> Self {
        RunIndex {
            runs: HashMap::new(),
            run_events: HashMap::new(),
        }
    }

    /// Insert new run
    pub fn insert(&mut self, run_id: RunId, metadata: RunMetadata) {
        self.runs.insert(run_id, metadata);
        self.run_events.insert(run_id, Vec::new());
    }

    /// Record event offset for run
    pub fn record_event(&mut self, run_id: RunId, offset: u64) {
        if let Some(offsets) = self.run_events.get_mut(&run_id) {
            offsets.push(offset);
        }
        if let Some(meta) = self.runs.get_mut(&run_id) {
            meta.event_count += 1;
        }
    }

    /// Get all event offsets for a run (for O(run size) replay)
    pub fn get_event_offsets(&self, run_id: RunId) -> Option<&[u64]> {
        self.run_events.get(&run_id).map(|v| v.as_slice())
    }

    /// Check if run exists
    pub fn exists(&self, run_id: RunId) -> bool {
        self.runs.contains_key(&run_id)
    }

    /// Get run metadata
    pub fn get(&self, run_id: RunId) -> Result<&RunMetadata, RunError> {
        self.runs.get(&run_id).ok_or(RunError::NotFound(run_id))
    }

    /// Update run metadata
    pub fn update<F>(&mut self, run_id: RunId, f: F) -> Result<(), RunError>
    where
        F: FnOnce(&mut RunMetadata),
    {
        let meta = self.runs.get_mut(&run_id).ok_or(RunError::NotFound(run_id))?;
        f(meta);
        Ok(())
    }

    /// List all runs
    pub fn list(&self) -> Vec<&RunMetadata> {
        self.runs.values().collect()
    }

    /// Find orphaned runs (Active status but should be Orphaned)
    pub fn find_orphaned(&mut self) -> Vec<RunId> {
        self.runs
            .iter()
            .filter(|(_, meta)| meta.status == RunStatus::Active)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Mark runs as orphaned
    pub fn mark_orphaned(&mut self, run_ids: &[RunId]) {
        for run_id in run_ids {
            if let Some(meta) = self.runs.get_mut(run_id) {
                meta.mark_orphaned();
            }
        }
    }
}
```

### Acceptance Criteria

- [ ] Tracks event offsets per run
- [ ] O(run size) lookup for replay
- [ ] Supports metadata updates
- [ ] List and search runs

### Complete Story

```bash
./scripts/complete-story.sh 368
```

---

## Story #369: replay_run() -> ReadOnlyView

**GitHub Issue**: [#369](https://github.com/anibjoshi/in-mem/issues/369)
**Estimated Time**: 4 hours
**Dependencies**: Stories #367, #368

### Start Story

```bash
gh issue view 369
./scripts/start-story.sh 43 369 replay-run
```

### Implementation

Create `crates/engine/src/replay.rs`:

```rust
//! Deterministic replay engine
//!
//! CRITICAL: Replay NEVER mutates the canonical store.
//! ReadOnlyView is derived, not authoritative.

use std::collections::HashMap;

/// Read-only view from replay
///
/// This is a derived view, not a new source of truth.
/// It does NOT persist and does NOT mutate the canonical store.
pub struct ReadOnlyView {
    /// Run this view is for
    pub run_id: RunId,
    /// KV state at run end
    kv_state: HashMap<Key, Value>,
    /// JSON state at run end
    json_state: HashMap<Key, JsonDoc>,
    /// Events during run
    event_state: Vec<Event>,
    /// State values at run end
    state_state: HashMap<Key, StateValue>,
    /// Traces during run
    trace_state: Vec<Span>,
}

impl ReadOnlyView {
    pub fn new(run_id: RunId) -> Self {
        ReadOnlyView {
            run_id,
            kv_state: HashMap::new(),
            json_state: HashMap::new(),
            event_state: Vec::new(),
            state_state: HashMap::new(),
            trace_state: Vec::new(),
        }
    }

    /// Get KV value
    pub fn get_kv(&self, key: &Key) -> Option<&Value> {
        self.kv_state.get(key)
    }

    /// Get JSON document
    pub fn get_json(&self, key: &Key) -> Option<&JsonDoc> {
        self.json_state.get(key)
    }

    /// Get events
    pub fn events(&self) -> &[Event] {
        &self.event_state
    }

    /// Get state value
    pub fn get_state(&self, key: &Key) -> Option<&StateValue> {
        self.state_state.get(key)
    }

    /// Get traces
    pub fn traces(&self) -> &[Span] {
        &self.trace_state
    }

    /// List all KV keys
    pub fn kv_keys(&self) -> impl Iterator<Item = &Key> {
        self.kv_state.keys()
    }

    /// List all JSON keys
    pub fn json_keys(&self) -> impl Iterator<Item = &Key> {
        self.json_state.keys()
    }
}

impl Database {
    /// Replay a run and return read-only view
    ///
    /// CRITICAL: This does NOT mutate the canonical store.
    /// The returned view is derived, not authoritative.
    pub fn replay_run(&self, run_id: RunId) -> Result<ReadOnlyView, ReplayError> {
        // Check run exists
        let status = self.run_status(run_id)?;
        if status == RunStatus::NotFound {
            return Err(ReplayError::RunNotFound(run_id));
        }

        // Get run's event offsets
        let offsets = self
            .run_index
            .get_event_offsets(run_id)
            .ok_or(ReplayError::RunNotFound(run_id))?;

        // Build state by replaying events
        let mut view = ReadOnlyView::new(run_id);

        for offset in offsets {
            let event = self.event_log.read_at(*offset)?;
            Self::apply_event_to_view(&mut view, &event)?;
        }

        Ok(view)
    }

    fn apply_event_to_view(view: &mut ReadOnlyView, event: &RunEvent) -> Result<(), ReplayError> {
        match event {
            RunEvent::KvPut { key, value } => {
                view.kv_state.insert(key.clone(), value.clone());
            }
            RunEvent::KvDelete { key } => {
                view.kv_state.remove(key);
            }
            RunEvent::JsonSet { key, doc } => {
                view.json_state.insert(key.clone(), doc.clone());
            }
            RunEvent::JsonDelete { key } => {
                view.json_state.remove(key);
            }
            RunEvent::JsonPatch { key, patch } => {
                if let Some(doc) = view.json_state.get_mut(key) {
                    doc.apply_patch(patch)?;
                }
            }
            RunEvent::StateSet { key, value } => {
                view.state_state.insert(key.clone(), value.clone());
            }
            RunEvent::EventAppend { event } => {
                view.event_state.push(event.clone());
            }
            RunEvent::TraceSpan { span } => {
                view.trace_state.push(span.clone());
            }
            _ => {}
        }
        Ok(())
    }
}

/// Replay errors
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("Run not found: {0:?}")]
    RunNotFound(RunId),

    #[error("Event log error: {0}")]
    EventLog(String),
}
```

### Acceptance Criteria

- [ ] Returns ReadOnlyView (P3: derived view)
- [ ] Does NOT mutate canonical store (P2: side-effect free)
- [ ] Same run_id = same view (P5: deterministic)
- [ ] O(run size) complexity via event offsets

### Complete Story

```bash
./scripts/complete-story.sh 369
```

---

## Story #370: diff_runs() Key-Level Comparison

**GitHub Issue**: [#370](https://github.com/anibjoshi/in-mem/issues/370)
**Estimated Time**: 3 hours
**Dependencies**: Story #369

### Start Story

```bash
gh issue view 370
./scripts/start-story.sh 43 370 diff-runs
```

### Implementation

```rust
/// Diff between two runs (key-level)
#[derive(Debug)]
pub struct RunDiff {
    /// Run A
    pub run_a: RunId,
    /// Run B
    pub run_b: RunId,
    /// Keys added in B (not in A)
    pub added: Vec<DiffEntry>,
    /// Keys removed in B (in A but not B)
    pub removed: Vec<DiffEntry>,
    /// Keys modified (different values)
    pub modified: Vec<DiffEntry>,
}

#[derive(Debug)]
pub struct DiffEntry {
    /// Key that changed
    pub key: Key,
    /// Primitive type
    pub primitive: PrimitiveKind,
    /// Value in run A (if present)
    pub value_a: Option<String>,
    /// Value in run B (if present)
    pub value_b: Option<String>,
}

impl Database {
    /// Compare two runs at key level
    pub fn diff_runs(&self, run_a: RunId, run_b: RunId) -> Result<RunDiff, ReplayError> {
        // Replay both runs
        let view_a = self.replay_run(run_a)?;
        let view_b = self.replay_run(run_b)?;

        let mut diff = RunDiff {
            run_a,
            run_b,
            added: Vec::new(),
            removed: Vec::new(),
            modified: Vec::new(),
        };

        // Compare KV state
        Self::diff_maps(&view_a.kv_state, &view_b.kv_state, PrimitiveKind::Kv, &mut diff);

        // Compare JSON state
        Self::diff_maps(&view_a.json_state, &view_b.json_state, PrimitiveKind::Json, &mut diff);

        // Compare State state
        Self::diff_maps(&view_a.state_state, &view_b.state_state, PrimitiveKind::State, &mut diff);

        Ok(diff)
    }

    fn diff_maps<V: std::fmt::Debug + PartialEq>(
        map_a: &HashMap<Key, V>,
        map_b: &HashMap<Key, V>,
        primitive: PrimitiveKind,
        diff: &mut RunDiff,
    ) {
        // Added: in B but not A
        for (key, value_b) in map_b {
            if !map_a.contains_key(key) {
                diff.added.push(DiffEntry {
                    key: key.clone(),
                    primitive,
                    value_a: None,
                    value_b: Some(format!("{:?}", value_b)),
                });
            }
        }

        // Removed: in A but not B
        for (key, value_a) in map_a {
            if !map_b.contains_key(key) {
                diff.removed.push(DiffEntry {
                    key: key.clone(),
                    primitive,
                    value_a: Some(format!("{:?}", value_a)),
                    value_b: None,
                });
            }
        }

        // Modified: in both but different
        for (key, value_a) in map_a {
            if let Some(value_b) = map_b.get(key) {
                if value_a != value_b {
                    diff.modified.push(DiffEntry {
                        key: key.clone(),
                        primitive,
                        value_a: Some(format!("{:?}", value_a)),
                        value_b: Some(format!("{:?}", value_b)),
                    });
                }
            }
        }
    }
}
```

### Acceptance Criteria

- [ ] Compares KV, JSON, State at key level
- [ ] Reports added, removed, modified keys
- [ ] Values stringified for display
- [ ] Uses replay internally

### Complete Story

```bash
./scripts/complete-story.sh 370
```

---

## Story #371: Orphaned Run Detection

**GitHub Issue**: [#371](https://github.com/anibjoshi/in-mem/issues/371)
**Estimated Time**: 2 hours
**Dependencies**: Story #368

### Start Story

```bash
gh issue view 371
./scripts/start-story.sh 43 371 orphaned-runs
```

### Implementation

```rust
impl Database {
    /// Find orphaned runs (no end marker)
    ///
    /// Runs that have begin_run() but no end_run() are orphaned.
    /// This typically indicates a crash during the run.
    pub fn find_orphaned_runs(&self) -> Vec<RunId> {
        self.run_index.find_orphaned()
    }

    /// Mark orphaned runs after recovery
    ///
    /// Called during recovery to mark runs without end markers.
    pub fn mark_orphaned_runs(&mut self) {
        let orphaned = self.run_index.find_orphaned();
        if !orphaned.is_empty() {
            tracing::warn!("Found {} orphaned runs", orphaned.len());
            for run_id in &orphaned {
                tracing::warn!("  Orphaned run: {:?}", run_id);
            }
            self.run_index.mark_orphaned(&orphaned);
        }
    }

    /// Get run status
    pub fn run_status(&self, run_id: RunId) -> Result<RunStatus, RunError> {
        match self.run_index.get(run_id) {
            Ok(meta) => Ok(meta.status),
            Err(RunError::NotFound(_)) => Ok(RunStatus::NotFound),
            Err(e) => Err(e),
        }
    }

    /// List orphaned runs
    pub fn orphaned_runs(&self) -> Vec<&RunMetadata> {
        self.run_index
            .list()
            .into_iter()
            .filter(|m| m.status == RunStatus::Orphaned)
            .collect()
    }
}
```

### Acceptance Criteria

- [ ] Detects runs without end marker
- [ ] Marks as Orphaned status
- [ ] Logs warnings during recovery
- [ ] API to list orphaned runs

### Complete Story

```bash
./scripts/complete-story.sh 371
```

---

## Epic 43 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-engine -- replay
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Replay Invariants

```rust
#[test]
fn test_replay_deterministic() {
    // P5: Same inputs = same view
    let view1 = db.replay_run(run_id)?;
    let view2 = db.replay_run(run_id)?;
    assert_eq!(view1.kv_state, view2.kv_state);
}

#[test]
fn test_replay_side_effect_free() {
    // P2: Does not mutate canonical store
    let before = db.kv.list_all()?;
    let _ = db.replay_run(run_id)?;
    let after = db.kv.list_all()?;
    assert_eq!(before, after);
}
```

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-43-run-replay -m "Epic 43: Run Lifecycle & Replay complete

Delivered:
- RunStatus and RunMetadata types
- begin_run() and end_run() implementations
- RunIndex with event offset tracking
- replay_run() returning ReadOnlyView
- diff_runs() for key-level comparison
- Orphaned run detection

All replay invariants (P1-P6) verified.

Stories: #365, #366, #367, #368, #369, #370, #371
"
git push origin develop
gh issue close 341 --comment "Epic 43: Run Lifecycle & Replay - COMPLETE"
```

---

## Summary

Epic 43 implements run lifecycle and deterministic replay. The key insight is that **replay is interpretation, not mutation**. The ReadOnlyView is derived from the EventLog, not a new source of truth.
