# Epic 43: Run Lifecycle & Replay

**Goal**: Implement run lifecycle and deterministic replay

**Dependencies**: Epic 41 (Crash Recovery), Epic 42 (WAL Enhancement)

---

## Scope

- Run status and metadata types
- begin_run() and end_run() lifecycle methods
- RunIndex event offset tracking for efficient replay
- replay_run() returning ReadOnlyView
- diff_runs() for key-level comparison
- Orphaned run detection

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #310 | RunStatus Enum and RunMetadata Type | FOUNDATION |
| #311 | begin_run() Implementation | CRITICAL |
| #312 | end_run() Implementation | CRITICAL |
| #313 | RunIndex Event Offset Tracking | CRITICAL |
| #314 | replay_run() -> ReadOnlyView | CRITICAL |
| #315 | diff_runs() Key-Level Comparison | HIGH |
| #316 | Orphaned Run Detection | HIGH |

---

## Story #310: RunStatus Enum and RunMetadata Type

**File**: `crates/core/src/run_types.rs` (NEW)

**Deliverable**: Run lifecycle types

### Implementation

```rust
use crate::types::RunId;

/// Status of a run
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    /// Run is currently active
    Active,
    /// Run completed normally
    Completed,
    /// Run was never ended (orphaned)
    Orphaned,
    /// Run doesn't exist
    NotFound,
}

/// Metadata about a run
#[derive(Debug, Clone)]
pub struct RunMetadata {
    /// Run identifier
    pub run_id: RunId,
    /// Current status
    pub status: RunStatus,
    /// When run started (microseconds since epoch)
    pub started_at: u64,
    /// When run ended (microseconds since epoch), if completed
    pub ended_at: Option<u64>,
    /// Number of events in this run
    pub event_count: u64,
    /// Optional description or label
    pub description: Option<String>,
}

impl RunMetadata {
    pub fn new(run_id: RunId) -> Self {
        RunMetadata {
            run_id,
            status: RunStatus::Active,
            started_at: now_micros(),
            ended_at: None,
            event_count: 0,
            description: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Duration in microseconds (if ended)
    pub fn duration_micros(&self) -> Option<u64> {
        self.ended_at.map(|end| end - self.started_at)
    }

    /// Check if run is still active
    pub fn is_active(&self) -> bool {
        self.status == RunStatus::Active
    }
}
```

### Acceptance Criteria

- [ ] RunStatus has Active, Completed, Orphaned, NotFound
- [ ] RunMetadata has all required fields
- [ ] Helper methods for common checks
- [ ] Clone, Debug implemented

---

## Story #311: begin_run() Implementation

**File**: `crates/engine/src/database.rs`

**Deliverable**: Begin a new run with WAL entry

### Implementation

```rust
impl Database {
    /// Begin a new run
    ///
    /// Creates run metadata and writes to WAL/EventLog.
    /// Returns error if run already exists.
    pub fn begin_run(&self, run_id: RunId) -> Result<(), RunError> {
        // Check run doesn't already exist
        if self.run_index.exists(run_id)? {
            return Err(RunError::AlreadyExists(run_id));
        }

        // Create metadata
        let metadata = RunMetadata::new(run_id);

        // Write WAL entry
        let payload = bincode::serialize(&metadata)?;
        let entry = WalEntry {
            entry_type: WalEntryType::RunBegin,
            version: 1,
            tx_id: TxId::nil(),  // Run lifecycle is not transactional
            payload,
        };
        self.wal.write_entry(&entry)?;

        // Update run index
        self.run_index.insert(metadata)?;

        // Write to EventLog (semantic history)
        self.event_log.append_system_event(
            run_id,
            SystemEvent::RunStarted { run_id },
        )?;

        tracing::info!("Run started: {:?}", run_id);

        Ok(())
    }

    /// Begin run with description
    pub fn begin_run_with_description(
        &self,
        run_id: RunId,
        description: impl Into<String>,
    ) -> Result<(), RunError> {
        // Similar to begin_run but with description
        if self.run_index.exists(run_id)? {
            return Err(RunError::AlreadyExists(run_id));
        }

        let metadata = RunMetadata::new(run_id)
            .with_description(description);

        // ... rest same as begin_run
        Ok(())
    }
}

/// Run lifecycle errors
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

    #[error("Serialization error: {0}")]
    Serialize(#[from] bincode::Error),
}
```

### Acceptance Criteria

- [ ] Creates run metadata with Active status
- [ ] Writes WAL entry (RunBegin)
- [ ] Updates RunIndex
- [ ] Writes to EventLog for replay
- [ ] Returns error if run exists

---

## Story #312: end_run() Implementation

**File**: `crates/engine/src/database.rs`

**Deliverable**: End a run with WAL entry

### Implementation

```rust
impl Database {
    /// End a run
    ///
    /// Marks run as completed in WAL/EventLog.
    /// Returns error if run not active.
    pub fn end_run(&self, run_id: RunId) -> Result<(), RunError> {
        // Check run exists and is active
        let metadata = self.run_index.get(run_id)?
            .ok_or(RunError::NotFound(run_id))?;

        if metadata.status != RunStatus::Active {
            return Err(RunError::NotActive(run_id));
        }

        // Write WAL entry
        let payload = run_id.as_bytes().to_vec();
        let entry = WalEntry {
            entry_type: WalEntryType::RunEnd,
            version: 1,
            tx_id: TxId::nil(),
            payload,
        };
        self.wal.write_entry(&entry)?;

        // Update run index
        self.run_index.update(run_id, |meta| {
            meta.status = RunStatus::Completed;
            meta.ended_at = Some(now_micros());
        })?;

        // Write to EventLog
        self.event_log.append_system_event(
            run_id,
            SystemEvent::RunEnded { run_id },
        )?;

        tracing::info!("Run ended: {:?}", run_id);

        Ok(())
    }

    /// Abort a run (mark as failed)
    pub fn abort_run(&self, run_id: RunId, reason: &str) -> Result<(), RunError> {
        let metadata = self.run_index.get(run_id)?
            .ok_or(RunError::NotFound(run_id))?;

        if metadata.status != RunStatus::Active {
            return Err(RunError::NotActive(run_id));
        }

        // Write WAL entry with reason
        let payload = bincode::serialize(&(run_id, reason))?;
        let entry = WalEntry {
            entry_type: WalEntryType::RunEnd,  // Same type, different payload
            version: 1,
            tx_id: TxId::nil(),
            payload,
        };
        self.wal.write_entry(&entry)?;

        // Update status
        self.run_index.update(run_id, |meta| {
            meta.status = RunStatus::Completed;  // Still completed, but can check reason
            meta.ended_at = Some(now_micros());
        })?;

        tracing::warn!("Run aborted: {:?}, reason: {}", run_id, reason);

        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] Marks run as Completed
- [ ] Writes WAL entry (RunEnd)
- [ ] Updates RunIndex with ended_at
- [ ] Returns error if run not active
- [ ] abort_run() handles failure case

---

## Story #313: RunIndex Event Offset Tracking

**File**: `crates/primitives/src/run_index.rs`

**Deliverable**: Track event offsets for efficient replay

### Implementation

```rust
use std::collections::HashMap;

/// Run index with event tracking for replay
pub struct RunIndex {
    /// Run metadata
    runs: HashMap<RunId, RunMetadata>,
    /// Run -> EventLog offsets for fast replay
    event_offsets: HashMap<RunId, Vec<u64>>,
    /// Total event count per run
    event_counts: HashMap<RunId, u64>,
}

impl RunIndex {
    pub fn new() -> Self {
        RunIndex {
            runs: HashMap::new(),
            event_offsets: HashMap::new(),
            event_counts: HashMap::new(),
        }
    }

    /// Record an event offset for a run
    ///
    /// Called when events are written to EventLog.
    pub fn record_event(&mut self, run_id: RunId, event_offset: u64) {
        self.event_offsets
            .entry(run_id)
            .or_insert_with(Vec::new)
            .push(event_offset);

        *self.event_counts.entry(run_id).or_insert(0) += 1;
    }

    /// Get event offsets for a run (for O(run size) replay)
    pub fn get_event_offsets(&self, run_id: RunId) -> Option<&[u64]> {
        self.event_offsets.get(&run_id).map(|v| v.as_slice())
    }

    /// Get event count for a run
    pub fn event_count(&self, run_id: RunId) -> u64 {
        self.event_counts.get(&run_id).copied().unwrap_or(0)
    }

    /// List runs with event counts
    pub fn list_runs(&self) -> Vec<(RunId, &RunMetadata)> {
        self.runs.iter().map(|(id, meta)| (*id, meta)).collect()
    }

    /// Insert run metadata
    pub fn insert(&mut self, metadata: RunMetadata) -> Result<(), RunError> {
        if self.runs.contains_key(&metadata.run_id) {
            return Err(RunError::AlreadyExists(metadata.run_id));
        }
        self.runs.insert(metadata.run_id, metadata);
        Ok(())
    }

    /// Update run metadata
    pub fn update<F>(&mut self, run_id: RunId, f: F) -> Result<(), RunError>
    where
        F: FnOnce(&mut RunMetadata),
    {
        let meta = self.runs.get_mut(&run_id)
            .ok_or(RunError::NotFound(run_id))?;
        f(meta);
        Ok(())
    }

    /// Check if run exists
    pub fn exists(&self, run_id: RunId) -> Result<bool, RunError> {
        Ok(self.runs.contains_key(&run_id))
    }

    /// Get run metadata
    pub fn get(&self, run_id: RunId) -> Result<Option<RunMetadata>, RunError> {
        Ok(self.runs.get(&run_id).cloned())
    }
}
```

### Acceptance Criteria

- [ ] Records event offsets per run
- [ ] get_event_offsets() returns offsets for replay
- [ ] Event count tracking
- [ ] O(1) lookup by run_id

---

## Story #314: replay_run() -> ReadOnlyView

**File**: `crates/engine/src/replay.rs` (NEW)

**Deliverable**: Deterministic replay returning read-only view

### Implementation

```rust
use std::collections::HashMap;

/// Read-only view of run state (result of replay)
///
/// IMPORTANT: This does NOT mutate the canonical store.
/// It is a derived view for inspection.
pub struct ReadOnlyView {
    /// Run this view is for
    pub run_id: RunId,
    /// KV state at run end
    kv_state: HashMap<Key, Value>,
    /// JSON state at run end
    json_state: HashMap<Key, JsonDoc>,
    /// Events during run
    event_state: Vec<Event>,
    /// State cells at run end
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

    // Read-only accessors

    pub fn get_kv(&self, key: &Key) -> Option<&Value> {
        self.kv_state.get(key)
    }

    pub fn get_json(&self, key: &Key) -> Option<&JsonDoc> {
        self.json_state.get(key)
    }

    pub fn events(&self) -> &[Event] {
        &self.event_state
    }

    pub fn get_state(&self, key: &Key) -> Option<&StateValue> {
        self.state_state.get(key)
    }

    pub fn traces(&self) -> &[Span] {
        &self.trace_state
    }

    pub fn kv_keys(&self) -> impl Iterator<Item = &Key> {
        self.kv_state.keys()
    }

    pub fn json_keys(&self) -> impl Iterator<Item = &Key> {
        self.json_state.keys()
    }

    pub fn state_keys(&self) -> impl Iterator<Item = &Key> {
        self.state_state.keys()
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

        // Get event offsets for this run
        let offsets = self.run_index.get_event_offsets(run_id)
            .ok_or(ReplayError::NoEvents(run_id))?;

        // Build view by replaying events
        let mut view = ReadOnlyView::new(run_id);

        for &offset in offsets {
            let event = self.event_log.read_at_offset(offset)?;
            Self::apply_event_to_view(&mut view, event)?;
        }

        Ok(view)
    }

    fn apply_event_to_view(view: &mut ReadOnlyView, event: RunEvent) -> Result<(), ReplayError> {
        match event {
            RunEvent::KvPut { key, value } => {
                view.kv_state.insert(key, value);
            }
            RunEvent::KvDelete { key } => {
                view.kv_state.remove(&key);
            }
            RunEvent::JsonSet { key, doc } => {
                view.json_state.insert(key, doc);
            }
            RunEvent::JsonDelete { key } => {
                view.json_state.remove(&key);
            }
            RunEvent::JsonPatch { key, patch } => {
                if let Some(doc) = view.json_state.get_mut(&key) {
                    doc.apply_patch(&patch)?;
                }
            }
            RunEvent::StateSet { key, value } => {
                view.state_state.insert(key, value);
            }
            RunEvent::EventAppend { event } => {
                view.event_state.push(event);
            }
            RunEvent::TraceSpan { span } => {
                view.trace_state.push(span);
            }
            _ => {}  // System events don't affect view
        }
        Ok(())
    }
}

/// Replay errors
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("Run not found: {0:?}")]
    RunNotFound(RunId),

    #[error("No events for run: {0:?}")]
    NoEvents(RunId),

    #[error("Event log error: {0}")]
    EventLog(String),

    #[error("Patch error: {0}")]
    Patch(String),
}
```

### Acceptance Criteria

- [ ] Returns ReadOnlyView (not mutable state)
- [ ] Does NOT mutate canonical store
- [ ] Uses event offsets for O(run size) replay
- [ ] Handles all event types
- [ ] Returns error for unknown run

---

## Story #315: diff_runs() Key-Level Comparison

**File**: `crates/engine/src/replay.rs`

**Deliverable**: Compare two runs at key level

### Implementation

```rust
/// Diff between two runs (key-level)
#[derive(Debug)]
pub struct RunDiff {
    /// Run A (baseline)
    pub run_a: RunId,
    /// Run B (comparison)
    pub run_b: RunId,
    /// Keys added in B (not in A)
    pub added: Vec<DiffEntry>,
    /// Keys removed in B (in A but not B)
    pub removed: Vec<DiffEntry>,
    /// Keys modified (different values)
    pub modified: Vec<DiffEntry>,
}

/// A single diff entry
#[derive(Debug)]
pub struct DiffEntry {
    /// Key that changed
    pub key: Key,
    /// Primitive type
    pub primitive: PrimitiveKind,
    /// Value in run A (stringified for display)
    pub value_a: Option<String>,
    /// Value in run B (stringified for display)
    pub value_b: Option<String>,
}

impl Database {
    /// Compare two runs at key level
    ///
    /// Shows what changed between run_a and run_b.
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
        Self::diff_maps(
            &view_a.kv_state,
            &view_b.kv_state,
            PrimitiveKind::Kv,
            &mut diff,
        );

        // Compare JSON state
        Self::diff_maps(
            &view_a.json_state,
            &view_b.json_state,
            PrimitiveKind::Json,
            &mut diff,
        );

        // Compare State state
        Self::diff_maps(
            &view_a.state_state,
            &view_b.state_state,
            PrimitiveKind::State,
            &mut diff,
        );

        Ok(diff)
    }

    fn diff_maps<V: std::fmt::Debug + PartialEq>(
        map_a: &HashMap<Key, V>,
        map_b: &HashMap<Key, V>,
        primitive: PrimitiveKind,
        diff: &mut RunDiff,
    ) {
        // Keys in B but not A (added)
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

        // Keys in A but not B (removed)
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

        // Keys in both but different values (modified)
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

impl RunDiff {
    /// Check if runs are identical
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }

    /// Total number of differences
    pub fn count(&self) -> usize {
        self.added.len() + self.removed.len() + self.modified.len()
    }

    /// Summary string
    pub fn summary(&self) -> String {
        format!(
            "Diff: {} added, {} removed, {} modified",
            self.added.len(),
            self.removed.len(),
            self.modified.len()
        )
    }
}
```

### Acceptance Criteria

- [ ] Compares KV, JSON, State at key level
- [ ] Reports added, removed, modified keys
- [ ] Values stringified for display
- [ ] is_empty() check for identical runs
- [ ] summary() for quick overview

---

## Story #316: Orphaned Run Detection

**File**: `crates/primitives/src/run_index.rs`

**Deliverable**: Detect runs without end markers

### Implementation

```rust
impl RunIndex {
    /// Detect orphaned runs (no end marker)
    ///
    /// An orphaned run is one that was started but never ended.
    /// This can happen if the process crashed.
    pub fn orphaned_runs(&self) -> Vec<RunId> {
        self.runs
            .iter()
            .filter(|(_, meta)| meta.status == RunStatus::Active)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Mark runs as orphaned
    ///
    /// Called during recovery for active runs.
    pub fn mark_orphaned(&mut self, run_id: RunId) -> Result<(), RunError> {
        let meta = self.runs.get_mut(&run_id)
            .ok_or(RunError::NotFound(run_id))?;

        if meta.status == RunStatus::Active {
            meta.status = RunStatus::Orphaned;
            tracing::warn!("Run marked as orphaned: {:?}", run_id);
        }

        Ok(())
    }

    /// Get runs by status
    pub fn runs_by_status(&self, status: RunStatus) -> Vec<RunId> {
        self.runs
            .iter()
            .filter(|(_, meta)| meta.status == status)
            .map(|(id, _)| *id)
            .collect()
    }
}

impl Database {
    /// List orphaned runs
    pub fn orphaned_runs(&self) -> Result<Vec<RunId>, RunError> {
        Ok(self.run_index.orphaned_runs())
    }

    /// Detect and mark orphaned runs during recovery
    pub fn detect_orphaned_runs(&self) -> Result<Vec<RunId>, RunError> {
        let orphaned = self.run_index.orphaned_runs();

        for run_id in &orphaned {
            self.run_index.mark_orphaned(*run_id)?;
        }

        if !orphaned.is_empty() {
            tracing::warn!("Detected {} orphaned runs", orphaned.len());
        }

        Ok(orphaned)
    }
}
```

### Acceptance Criteria

- [ ] orphaned_runs() returns Active runs
- [ ] mark_orphaned() updates status
- [ ] Called during recovery
- [ ] Logs warnings for orphaned runs
- [ ] runs_by_status() for filtering

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_lifecycle() {
        let db = test_db();
        let run_id = RunId::new();

        // Begin run
        db.begin_run(run_id).unwrap();
        assert_eq!(db.run_status(run_id).unwrap(), RunStatus::Active);

        // Do work
        db.kv.put(run_id, "key1", "value1").unwrap();

        // End run
        db.end_run(run_id).unwrap();
        assert_eq!(db.run_status(run_id).unwrap(), RunStatus::Completed);
    }

    #[test]
    fn test_replay_deterministic() {
        let db = test_db();
        let run_id = RunId::new();

        db.begin_run(run_id).unwrap();
        db.kv.put(run_id, "key1", "value1").unwrap();
        db.kv.put(run_id, "key2", "value2").unwrap();
        db.end_run(run_id).unwrap();

        // Replay twice
        let view1 = db.replay_run(run_id).unwrap();
        let view2 = db.replay_run(run_id).unwrap();

        // Must be identical
        assert_eq!(view1.kv_state, view2.kv_state);
    }

    #[test]
    fn test_replay_side_effect_free() {
        let db = test_db();
        let run_id = RunId::new();

        db.begin_run(run_id).unwrap();
        db.kv.put(run_id, "key1", "value1").unwrap();
        db.end_run(run_id).unwrap();

        // Get canonical state
        let before = db.kv.list_all().unwrap();

        // Replay
        let _view = db.replay_run(run_id).unwrap();

        // Canonical state unchanged
        let after = db.kv.list_all().unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn test_diff_runs() {
        let db = test_db();

        // Run A: keys 1, 2, 3
        let run_a = RunId::new();
        db.begin_run(run_a).unwrap();
        db.kv.put(run_a, "key1", "value1").unwrap();
        db.kv.put(run_a, "key2", "value2").unwrap();
        db.kv.put(run_a, "key3", "value3").unwrap();
        db.end_run(run_a).unwrap();

        // Run B: keys 2, 3 (modified), 4
        let run_b = RunId::new();
        db.begin_run(run_b).unwrap();
        db.kv.put(run_b, "key2", "value2").unwrap();
        db.kv.put(run_b, "key3", "modified").unwrap();
        db.kv.put(run_b, "key4", "value4").unwrap();
        db.end_run(run_b).unwrap();

        let diff = db.diff_runs(run_a, run_b).unwrap();

        assert_eq!(diff.added.len(), 1);   // key4
        assert_eq!(diff.removed.len(), 1); // key1
        assert_eq!(diff.modified.len(), 1); // key3
    }

    #[test]
    fn test_orphaned_detection() {
        let db = test_db();
        let run_id = RunId::new();

        // Begin but don't end
        db.begin_run(run_id).unwrap();

        // Should be detected as orphaned
        let orphaned = db.orphaned_runs().unwrap();
        assert!(orphaned.contains(&run_id));
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/run_types.rs` | CREATE - RunStatus, RunMetadata |
| `crates/core/src/lib.rs` | MODIFY - Export run_types |
| `crates/primitives/src/run_index.rs` | MODIFY - Event offset tracking |
| `crates/engine/src/replay.rs` | CREATE - ReplayEngine, ReadOnlyView, RunDiff |
| `crates/engine/src/database.rs` | MODIFY - begin_run, end_run, replay_run, diff_runs |
