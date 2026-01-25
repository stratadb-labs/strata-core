//! Run Lifecycle and Deterministic Replay
//!
//! This module implements run lifecycle management and deterministic replay.
//!
//! ## Replay Invariants (P1-P6)
//!
//! | # | Invariant | Meaning |
//! |---|-----------|---------|
//! | P1 | Pure function | Over (Snapshot, WAL, EventLog) |
//! | P2 | Side-effect free | Does not mutate canonical store |
//! | P3 | Derived view | Not a new source of truth |
//! | P4 | Does not persist | Unless explicitly materialized |
//! | P5 | Deterministic | Same inputs = Same view |
//! | P6 | Idempotent | Running twice produces identical view |
//!
//! **CRITICAL**: Replay NEVER writes to the canonical store.
//! ReadOnlyView is derived, not authoritative.
//!
//! ## Stories Implemented
//!
//! - begin_run() - Creates run metadata, writes WAL entry
//! - end_run() - Marks run completed, writes WAL entry
//! - RunIndex - Event offset tracking for O(run size) replay
//! - replay_run() - Returns ReadOnlyView
//! - diff_runs() - Key-level comparison
//! - Orphaned run detection

use strata_core::run_types::{RunEventOffsets, RunMetadata, RunStatus};
use strata_core::types::{Key, RunId};
use strata_core::value::Value;
use strata_core::{EntityRef, StrataError};
use std::collections::HashMap;
use thiserror::Error;

// ============================================================================
// Run Errors
// ============================================================================

/// Errors related to run lifecycle operations
#[derive(Debug, Error)]
pub enum RunError {
    /// Run already exists
    #[error("Run already exists: {0}")]
    AlreadyExists(RunId),

    /// Run not found
    #[error("Run not found: {0}")]
    NotFound(RunId),

    /// Run is not active
    #[error("Run not active: {0}")]
    NotActive(RunId),

    /// WAL error
    #[error("WAL error: {0}")]
    Wal(String),

    /// Storage error
    #[error("Storage error: {0}")]
    Storage(String),
}

impl From<strata_core::error::Error> for RunError {
    fn from(e: strata_core::error::Error) -> Self {
        RunError::Storage(e.to_string())
    }
}

// Conversion to StrataError
impl From<RunError> for StrataError {
    fn from(e: RunError) -> Self {
        match e {
            RunError::AlreadyExists(run_id) => StrataError::InvalidOperation {
                entity_ref: EntityRef::run(run_id),
                reason: format!("Run '{}' already exists", run_id),
            },
            RunError::NotFound(run_id) => StrataError::RunNotFound { run_id },
            RunError::NotActive(run_id) => StrataError::InvalidOperation {
                entity_ref: EntityRef::run(run_id),
                reason: "Run is not active".to_string(),
            },
            RunError::Wal(msg) => StrataError::Storage {
                message: format!("WAL error: {}", msg),
                source: None,
            },
            RunError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

// ============================================================================
// Run Index
// ============================================================================

/// Run index for tracking runs and their events
///
/// Maps runs to their metadata and event offsets for O(run size) replay.
#[derive(Debug, Default)]
pub struct RunIndex {
    /// Run metadata by run ID
    runs: HashMap<RunId, RunMetadata>,
    /// Event offsets by run ID (for O(run size) replay)
    run_events: HashMap<RunId, RunEventOffsets>,
}

impl RunIndex {
    /// Create a new empty run index
    pub fn new() -> Self {
        RunIndex {
            runs: HashMap::new(),
            run_events: HashMap::new(),
        }
    }

    /// Insert a new run
    pub fn insert(&mut self, run_id: RunId, metadata: RunMetadata) {
        self.runs.insert(run_id, metadata);
        self.run_events.insert(run_id, RunEventOffsets::new());
    }

    /// Check if a run exists
    pub fn exists(&self, run_id: RunId) -> bool {
        self.runs.contains_key(&run_id)
    }

    /// Get run metadata
    pub fn get(&self, run_id: RunId) -> Option<&RunMetadata> {
        self.runs.get(&run_id)
    }

    /// Get mutable run metadata
    pub fn get_mut(&mut self, run_id: RunId) -> Option<&mut RunMetadata> {
        self.runs.get_mut(&run_id)
    }

    /// Record an event offset for a run
    pub fn record_event(&mut self, run_id: RunId, offset: u64) {
        if let Some(offsets) = self.run_events.get_mut(&run_id) {
            offsets.push(offset);
        }
        if let Some(meta) = self.runs.get_mut(&run_id) {
            meta.increment_event_count();
        }
    }

    /// Get event offsets for a run (for O(run size) replay)
    pub fn get_event_offsets(&self, run_id: RunId) -> Option<&[u64]> {
        self.run_events.get(&run_id).map(|o| o.as_slice())
    }

    /// List all runs
    pub fn list(&self) -> Vec<&RunMetadata> {
        self.runs.values().collect()
    }

    /// List all run IDs
    pub fn list_run_ids(&self) -> Vec<RunId> {
        self.runs.keys().copied().collect()
    }

    /// Get run status
    pub fn status(&self, run_id: RunId) -> RunStatus {
        match self.runs.get(&run_id) {
            Some(meta) => meta.status,
            None => RunStatus::NotFound,
        }
    }

    /// Find runs that are still active (potential orphans after crash)
    pub fn find_active(&self) -> Vec<RunId> {
        self.runs
            .iter()
            .filter(|(_, meta)| meta.status.is_active())
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

    /// Count runs by status
    pub fn count_by_status(&self) -> HashMap<RunStatus, usize> {
        let mut counts = HashMap::new();
        for meta in self.runs.values() {
            *counts.entry(meta.status).or_insert(0) += 1;
        }
        counts
    }
}

// ============================================================================
// Read-Only View
// ============================================================================

/// Read-only view from replay
///
/// This is a derived view, NOT a new source of truth.
/// It does NOT persist and does NOT mutate the canonical store.
///
/// ## Replay Invariants
///
/// - P1: Pure function over (Snapshot, WAL, EventLog)
/// - P2: Side-effect free (does not mutate canonical store)
/// - P3: Derived view (not authoritative)
/// - P4: Does not persist (unless explicitly materialized)
/// - P5: Deterministic (same inputs = same view)
/// - P6: Idempotent (running twice produces identical view)
#[derive(Debug, Clone)]
pub struct ReadOnlyView {
    /// Run this view is for
    pub run_id: RunId,
    /// KV state at run end
    kv_state: HashMap<Key, Value>,
    /// Events during run (simplified as key-value pairs)
    events: Vec<(String, Value)>,
    /// Number of operations in this view
    operation_count: u64,
}

impl ReadOnlyView {
    /// Create a new empty read-only view
    pub fn new(run_id: RunId) -> Self {
        ReadOnlyView {
            run_id,
            kv_state: HashMap::new(),
            events: Vec::new(),
            operation_count: 0,
        }
    }

    /// Get a KV value
    pub fn get_kv(&self, key: &Key) -> Option<&Value> {
        self.kv_state.get(key)
    }

    /// Check if a KV key exists
    pub fn contains_kv(&self, key: &Key) -> bool {
        self.kv_state.contains_key(key)
    }

    /// List all KV keys
    pub fn kv_keys(&self) -> impl Iterator<Item = &Key> {
        self.kv_state.keys()
    }

    /// Get all KV entries
    pub fn kv_entries(&self) -> impl Iterator<Item = (&Key, &Value)> {
        self.kv_state.iter()
    }

    /// Get events
    pub fn events(&self) -> &[(String, Value)] {
        &self.events
    }

    /// Get event count
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Get operation count
    pub fn operation_count(&self) -> u64 {
        self.operation_count
    }

    /// Get number of KV entries
    pub fn kv_count(&self) -> usize {
        self.kv_state.len()
    }

    // Methods for building the view during replay
    // These are used by the replay implementation and tests

    /// Apply a KV put operation
    pub fn apply_kv_put(&mut self, key: Key, value: Value) {
        self.kv_state.insert(key, value);
        self.operation_count += 1;
    }

    /// Apply a KV delete operation
    pub fn apply_kv_delete(&mut self, key: &Key) {
        self.kv_state.remove(key);
        self.operation_count += 1;
    }

    /// Append an event
    pub fn append_event(&mut self, event_type: String, data: Value) {
        self.events.push((event_type, data));
        self.operation_count += 1;
    }
}

// ============================================================================
// Run Diff
// ============================================================================

/// Primitive kind for diff entries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiffPrimitiveKind {
    /// Key-value store
    Kv,
    /// Event log
    Event,
}

impl std::fmt::Display for DiffPrimitiveKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffPrimitiveKind::Kv => write!(f, "KV"),
            DiffPrimitiveKind::Event => write!(f, "Event"),
        }
    }
}

/// A single diff entry
#[derive(Debug, Clone)]
pub struct DiffEntry {
    /// Key that changed
    pub key: String,
    /// Primitive type
    pub primitive: DiffPrimitiveKind,
    /// Value in run A (if present)
    pub value_a: Option<String>,
    /// Value in run B (if present)
    pub value_b: Option<String>,
}

impl DiffEntry {
    /// Create a new diff entry for an added key
    pub fn added(key: String, primitive: DiffPrimitiveKind, value: String) -> Self {
        DiffEntry {
            key,
            primitive,
            value_a: None,
            value_b: Some(value),
        }
    }

    /// Create a new diff entry for a removed key
    pub fn removed(key: String, primitive: DiffPrimitiveKind, value: String) -> Self {
        DiffEntry {
            key,
            primitive,
            value_a: Some(value),
            value_b: None,
        }
    }

    /// Create a new diff entry for a modified key
    pub fn modified(
        key: String,
        primitive: DiffPrimitiveKind,
        old_value: String,
        new_value: String,
    ) -> Self {
        DiffEntry {
            key,
            primitive,
            value_a: Some(old_value),
            value_b: Some(new_value),
        }
    }
}

/// Diff between two runs at key level
#[derive(Debug, Clone)]
pub struct RunDiff {
    /// Run A (base)
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

impl RunDiff {
    /// Create a new empty diff
    pub fn new(run_a: RunId, run_b: RunId) -> Self {
        RunDiff {
            run_a,
            run_b,
            added: Vec::new(),
            removed: Vec::new(),
            modified: Vec::new(),
        }
    }

    /// Check if there are any differences
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }

    /// Total number of changes
    pub fn total_changes(&self) -> usize {
        self.added.len() + self.removed.len() + self.modified.len()
    }

    /// Get a summary string
    pub fn summary(&self) -> String {
        format!(
            "+{} -{} ~{} (total: {})",
            self.added.len(),
            self.removed.len(),
            self.modified.len(),
            self.total_changes()
        )
    }
}

/// Compare two ReadOnlyViews and produce a diff
pub fn diff_views(view_a: &ReadOnlyView, view_b: &ReadOnlyView) -> RunDiff {
    let mut diff = RunDiff::new(view_a.run_id, view_b.run_id);

    // Compare KV state
    diff_kv_maps(&view_a.kv_state, &view_b.kv_state, &mut diff);

    // Compare events (by count since events are append-only)
    if view_a.events.len() != view_b.events.len() {
        let a_count = view_a.events.len();
        let b_count = view_b.events.len();

        if b_count > a_count {
            // Events added in B
            for (event_type, data) in &view_b.events[a_count..] {
                diff.added.push(DiffEntry::added(
                    event_type.clone(),
                    DiffPrimitiveKind::Event,
                    format!("{:?}", data),
                ));
            }
        } else {
            // Events removed (shouldn't happen in normal operation, but detect it)
            for (event_type, data) in &view_a.events[b_count..] {
                diff.removed.push(DiffEntry::removed(
                    event_type.clone(),
                    DiffPrimitiveKind::Event,
                    format!("{:?}", data),
                ));
            }
        }
    }

    diff
}

fn diff_kv_maps(map_a: &HashMap<Key, Value>, map_b: &HashMap<Key, Value>, diff: &mut RunDiff) {
    // Added: in B but not A
    for (key, value_b) in map_b {
        if !map_a.contains_key(key) {
            let key_str = key
                .user_key_string()
                .unwrap_or_else(|| format!("{:?}", key.user_key));
            diff.added.push(DiffEntry::added(
                key_str,
                DiffPrimitiveKind::Kv,
                format!("{:?}", value_b),
            ));
        }
    }

    // Removed: in A but not B
    for (key, value_a) in map_a {
        if !map_b.contains_key(key) {
            let key_str = key
                .user_key_string()
                .unwrap_or_else(|| format!("{:?}", key.user_key));
            diff.removed.push(DiffEntry::removed(
                key_str,
                DiffPrimitiveKind::Kv,
                format!("{:?}", value_a),
            ));
        }
    }

    // Modified: in both but different
    for (key, value_a) in map_a {
        if let Some(value_b) = map_b.get(key) {
            if value_a != value_b {
                let key_str = key
                    .user_key_string()
                    .unwrap_or_else(|| format!("{:?}", key.user_key));
                diff.modified.push(DiffEntry::modified(
                    key_str,
                    DiffPrimitiveKind::Kv,
                    format!("{:?}", value_a),
                    format!("{:?}", value_b),
                ));
            }
        }
    }
}

// ============================================================================
// Replay Error
// ============================================================================

/// Errors during replay
#[derive(Debug, Error)]
pub enum ReplayError {
    /// Run not found
    #[error("Run not found: {0}")]
    RunNotFound(RunId),

    /// Event log error
    #[error("Event log error: {0}")]
    EventLog(String),

    /// WAL error
    #[error("WAL error: {0}")]
    Wal(String),

    /// Invalid operation during replay
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::Namespace;

    fn test_namespace() -> Namespace {
        Namespace::for_run(RunId::new())
    }

    // ========== RunIndex Tests ==========

    #[test]
    fn test_run_index_new() {
        let index = RunIndex::new();
        assert!(index.list().is_empty());
    }

    #[test]
    fn test_run_index_insert_and_get() {
        let mut index = RunIndex::new();
        let run_id = RunId::new();
        let metadata = RunMetadata::new(run_id, 1000, 0);

        index.insert(run_id, metadata.clone());

        assert!(index.exists(run_id));
        let retrieved = index.get(run_id).unwrap();
        assert_eq!(retrieved.run_id, run_id);
        assert_eq!(retrieved.status, RunStatus::Active);
    }

    #[test]
    fn test_run_index_status() {
        let mut index = RunIndex::new();
        let run_id = RunId::new();

        // Non-existent run
        assert_eq!(index.status(run_id), RunStatus::NotFound);

        // Insert run
        let metadata = RunMetadata::new(run_id, 1000, 0);
        index.insert(run_id, metadata);

        assert_eq!(index.status(run_id), RunStatus::Active);
    }

    #[test]
    fn test_run_index_record_event() {
        let mut index = RunIndex::new();
        let run_id = RunId::new();
        let metadata = RunMetadata::new(run_id, 1000, 0);

        index.insert(run_id, metadata);
        index.record_event(run_id, 100);
        index.record_event(run_id, 200);
        index.record_event(run_id, 300);

        let offsets = index.get_event_offsets(run_id).unwrap();
        assert_eq!(offsets, &[100, 200, 300]);

        let meta = index.get(run_id).unwrap();
        assert_eq!(meta.event_count, 3);
    }

    #[test]
    fn test_run_index_find_active() {
        let mut index = RunIndex::new();

        let run1 = RunId::new();
        let run2 = RunId::new();
        let run3 = RunId::new();

        index.insert(run1, RunMetadata::new(run1, 1000, 0));
        index.insert(run2, RunMetadata::new(run2, 2000, 100));
        index.insert(run3, RunMetadata::new(run3, 3000, 200));

        // Complete run2
        index.get_mut(run2).unwrap().complete(2500, 150);

        let active = index.find_active();
        assert_eq!(active.len(), 2);
        assert!(active.contains(&run1));
        assert!(active.contains(&run3));
        assert!(!active.contains(&run2));
    }

    #[test]
    fn test_run_index_mark_orphaned() {
        let mut index = RunIndex::new();

        let run1 = RunId::new();
        let run2 = RunId::new();

        index.insert(run1, RunMetadata::new(run1, 1000, 0));
        index.insert(run2, RunMetadata::new(run2, 2000, 100));

        index.mark_orphaned(&[run1]);

        assert_eq!(index.status(run1), RunStatus::Orphaned);
        assert_eq!(index.status(run2), RunStatus::Active);
    }

    // ========== ReadOnlyView Tests ==========

    #[test]
    fn test_read_only_view_new() {
        let run_id = RunId::new();
        let view = ReadOnlyView::new(run_id);

        assert_eq!(view.run_id, run_id);
        assert_eq!(view.kv_count(), 0);
        assert_eq!(view.event_count(), 0);
        assert_eq!(view.operation_count(), 0);
    }

    #[test]
    fn test_read_only_view_kv_operations() {
        let run_id = RunId::new();
        let ns = test_namespace();
        let mut view = ReadOnlyView::new(run_id);

        let key = Key::new_kv(ns.clone(), "test-key");
        let value = Value::String("test-value".into());

        // Apply put
        view.apply_kv_put(key.clone(), value.clone());
        assert_eq!(view.get_kv(&key), Some(&value));
        assert!(view.contains_kv(&key));
        assert_eq!(view.kv_count(), 1);
        assert_eq!(view.operation_count(), 1);

        // Apply delete
        view.apply_kv_delete(&key);
        assert_eq!(view.get_kv(&key), None);
        assert!(!view.contains_kv(&key));
        assert_eq!(view.kv_count(), 0);
        assert_eq!(view.operation_count(), 2);
    }

    #[test]
    fn test_read_only_view_events() {
        let run_id = RunId::new();
        let mut view = ReadOnlyView::new(run_id);

        view.append_event("UserCreated".into(), Value::String("alice".into()));
        view.append_event("UserUpdated".into(), Value::String("bob".into()));

        assert_eq!(view.event_count(), 2);
        assert_eq!(view.events()[0].0, "UserCreated");
        assert_eq!(view.events()[1].0, "UserUpdated");
    }

    // ========== RunDiff Tests ==========

    #[test]
    fn test_run_diff_empty() {
        let run_a = RunId::new();
        let run_b = RunId::new();

        let view_a = ReadOnlyView::new(run_a);
        let view_b = ReadOnlyView::new(run_b);

        let diff = diff_views(&view_a, &view_b);
        assert!(diff.is_empty());
        assert_eq!(diff.total_changes(), 0);
    }

    #[test]
    fn test_run_diff_added() {
        let run_a = RunId::new();
        let run_b = RunId::new();
        let ns = test_namespace();

        let view_a = ReadOnlyView::new(run_a);

        let mut view_b = ReadOnlyView::new(run_b);
        view_b.apply_kv_put(Key::new_kv(ns.clone(), "new-key"), Value::Int(42));

        let diff = diff_views(&view_a, &view_b);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 0);
        assert_eq!(diff.modified.len(), 0);
        assert_eq!(diff.added[0].key, "new-key");
    }

    #[test]
    fn test_run_diff_removed() {
        let run_a = RunId::new();
        let run_b = RunId::new();
        let ns = test_namespace();

        let mut view_a = ReadOnlyView::new(run_a);
        view_a.apply_kv_put(Key::new_kv(ns.clone(), "old-key"), Value::Int(42));

        let view_b = ReadOnlyView::new(run_b);

        let diff = diff_views(&view_a, &view_b);
        assert_eq!(diff.added.len(), 0);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.modified.len(), 0);
        assert_eq!(diff.removed[0].key, "old-key");
    }

    #[test]
    fn test_run_diff_modified() {
        let run_a = RunId::new();
        let run_b = RunId::new();
        let ns = test_namespace();

        let key = Key::new_kv(ns.clone(), "shared-key");

        let mut view_a = ReadOnlyView::new(run_a);
        view_a.apply_kv_put(key.clone(), Value::Int(1));

        let mut view_b = ReadOnlyView::new(run_b);
        view_b.apply_kv_put(key.clone(), Value::Int(2));

        let diff = diff_views(&view_a, &view_b);
        assert_eq!(diff.added.len(), 0);
        assert_eq!(diff.removed.len(), 0);
        assert_eq!(diff.modified.len(), 1);
        assert_eq!(diff.modified[0].key, "shared-key");
    }

    #[test]
    fn test_run_diff_summary() {
        let diff = RunDiff {
            run_a: RunId::new(),
            run_b: RunId::new(),
            added: vec![DiffEntry::added(
                "a".into(),
                DiffPrimitiveKind::Kv,
                "1".into(),
            )],
            removed: vec![
                DiffEntry::removed("b".into(), DiffPrimitiveKind::Kv, "2".into()),
                DiffEntry::removed("c".into(), DiffPrimitiveKind::Kv, "3".into()),
            ],
            modified: vec![DiffEntry::modified(
                "d".into(),
                DiffPrimitiveKind::Kv,
                "4".into(),
                "5".into(),
            )],
        };

        assert_eq!(diff.summary(), "+1 -2 ~1 (total: 4)");
    }

    // ========== Orphaned Run Detection Tests ==========

    #[test]
    fn test_orphaned_detection() {
        let mut index = RunIndex::new();

        // Create some runs
        let run1 = RunId::new();
        let run2 = RunId::new();
        let run3 = RunId::new();

        index.insert(run1, RunMetadata::new(run1, 1000, 0));
        index.insert(run2, RunMetadata::new(run2, 2000, 100));
        index.insert(run3, RunMetadata::new(run3, 3000, 200));

        // Complete run2 properly
        index.get_mut(run2).unwrap().complete(2500, 150);

        // Simulate crash - run1 and run3 are still active
        let active = index.find_active();
        assert_eq!(active.len(), 2);

        // Mark them as orphaned
        index.mark_orphaned(&active);

        // Verify
        assert_eq!(index.status(run1), RunStatus::Orphaned);
        assert_eq!(index.status(run2), RunStatus::Completed);
        assert_eq!(index.status(run3), RunStatus::Orphaned);
    }

    #[test]
    fn test_count_by_status() {
        let mut index = RunIndex::new();

        // Create runs with different states
        for _ in 0..3 {
            let run_id = RunId::new();
            index.insert(run_id, RunMetadata::new(run_id, 1000, 0));
        }

        for _ in 0..2 {
            let run_id = RunId::new();
            let mut meta = RunMetadata::new(run_id, 1000, 0);
            meta.complete(2000, 100);
            index.insert(run_id, meta);
        }

        let counts = index.count_by_status();
        assert_eq!(counts.get(&RunStatus::Active), Some(&3));
        assert_eq!(counts.get(&RunStatus::Completed), Some(&2));
    }
}
