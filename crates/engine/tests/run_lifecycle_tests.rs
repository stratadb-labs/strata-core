//! Integration tests for Run Lifecycle (Epic 43)
//!
//! Tests for:
//! - RunStatus and RunMetadata types
//! - RunIndex event offset tracking
//! - ReadOnlyView
//! - diff_runs() key-level comparison
//! - Orphaned run detection

use strata_core::run_types::{RunMetadata, RunStatus};
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_core::PrimitiveType;
use strata_engine::{diff_views, DiffEntry, ReadOnlyView, RunDiff, ReplayRunIndex as RunIndex};

// ============================================================================
// RunStatus and RunMetadata Tests
// ============================================================================

#[test]
fn test_run_status_values() {
    // Test all status values
    let active = RunStatus::Active;
    let completed = RunStatus::Completed;
    let orphaned = RunStatus::Orphaned;
    let not_found = RunStatus::NotFound;

    assert!(active.is_active());
    assert!(!active.is_completed());
    assert!(!active.is_orphaned());
    assert!(active.exists());

    assert!(!completed.is_active());
    assert!(completed.is_completed());
    assert!(!completed.is_orphaned());
    assert!(completed.exists());

    assert!(!orphaned.is_active());
    assert!(!orphaned.is_completed());
    assert!(orphaned.is_orphaned());
    assert!(orphaned.exists());

    assert!(!not_found.is_active());
    assert!(!not_found.is_completed());
    assert!(!not_found.is_orphaned());
    assert!(!not_found.exists());
}

#[test]
fn test_run_metadata_lifecycle() {
    let run_id = RunId::new();
    let started_at = 1000000u64;
    let begin_offset = 0u64;

    // Create new metadata
    let mut meta = RunMetadata::new(run_id, started_at, begin_offset);
    assert_eq!(meta.run_id, run_id);
    assert_eq!(meta.status, RunStatus::Active);
    assert_eq!(meta.started_at, started_at);
    assert_eq!(meta.ended_at, None);
    assert_eq!(meta.event_count, 0);
    assert_eq!(meta.begin_wal_offset, begin_offset);
    assert_eq!(meta.end_wal_offset, None);
    assert_eq!(meta.duration_micros(), None);

    // Increment event count
    meta.increment_event_count();
    meta.increment_event_count();
    assert_eq!(meta.event_count, 2);

    // Complete the run
    let ended_at = 2000000u64;
    let end_offset = 500u64;
    meta.complete(ended_at, end_offset);

    assert_eq!(meta.status, RunStatus::Completed);
    assert_eq!(meta.ended_at, Some(ended_at));
    assert_eq!(meta.end_wal_offset, Some(end_offset));
    assert_eq!(meta.duration_micros(), Some(1000000));
}

#[test]
fn test_run_metadata_orphaned() {
    let run_id = RunId::new();
    let mut meta = RunMetadata::new(run_id, 1000, 0);

    meta.mark_orphaned();

    assert_eq!(meta.status, RunStatus::Orphaned);
    assert!(meta.status.is_orphaned());
}

// ============================================================================
// RunIndex Event Offset Tracking
// ============================================================================

#[test]
fn test_run_index_basic_operations() {
    let mut index = RunIndex::new();

    let run_id = RunId::new();
    let meta = RunMetadata::new(run_id, 1000, 0);

    // Insert run
    index.insert(run_id, meta);
    assert!(index.exists(run_id));
    assert_eq!(index.status(run_id), RunStatus::Active);

    // Non-existent run
    let other_run = RunId::new();
    assert!(!index.exists(other_run));
    assert_eq!(index.status(other_run), RunStatus::NotFound);
}

#[test]
fn test_run_index_event_tracking() {
    let mut index = RunIndex::new();

    let run_id = RunId::new();
    let meta = RunMetadata::new(run_id, 1000, 0);
    index.insert(run_id, meta);

    // Record events
    index.record_event(run_id, 100);
    index.record_event(run_id, 200);
    index.record_event(run_id, 300);

    // Verify offsets
    let offsets = index.get_event_offsets(run_id).unwrap();
    assert_eq!(offsets, &[100, 200, 300]);

    // Verify event count in metadata
    let meta = index.get(run_id).unwrap();
    assert_eq!(meta.event_count, 3);
}

#[test]
fn test_run_index_multiple_runs() {
    let mut index = RunIndex::new();

    // Create multiple runs
    let run1 = RunId::new();
    let run2 = RunId::new();
    let run3 = RunId::new();

    index.insert(run1, RunMetadata::new(run1, 1000, 0));
    index.insert(run2, RunMetadata::new(run2, 2000, 100));
    index.insert(run3, RunMetadata::new(run3, 3000, 200));

    // Record events for different runs
    index.record_event(run1, 10);
    index.record_event(run1, 20);
    index.record_event(run2, 30);
    index.record_event(run3, 40);
    index.record_event(run3, 50);
    index.record_event(run3, 60);

    // Verify isolation
    assert_eq!(index.get_event_offsets(run1).unwrap(), &[10, 20]);
    assert_eq!(index.get_event_offsets(run2).unwrap(), &[30]);
    assert_eq!(index.get_event_offsets(run3).unwrap(), &[40, 50, 60]);

    // List all runs
    let runs = index.list();
    assert_eq!(runs.len(), 3);

    let run_ids = index.list_run_ids();
    assert!(run_ids.contains(&run1));
    assert!(run_ids.contains(&run2));
    assert!(run_ids.contains(&run3));
}

// ============================================================================
// ReadOnlyView Tests
// ============================================================================

fn test_namespace() -> Namespace {
    Namespace::for_run(RunId::new())
}

#[test]
fn test_read_only_view_creation() {
    let run_id = RunId::new();
    let view = ReadOnlyView::new(run_id);

    assert_eq!(view.run_id, run_id);
    assert_eq!(view.kv_count(), 0);
    assert_eq!(view.event_count(), 0);
    assert_eq!(view.operation_count(), 0);
}

#[test]
fn test_read_only_view_kv_state() {
    let run_id = RunId::new();
    let ns = test_namespace();
    let mut view = ReadOnlyView::new(run_id);

    // Build up state
    let key1 = Key::new_kv(ns.clone(), "key1");
    let key2 = Key::new_kv(ns.clone(), "key2");

    view.apply_kv_put(key1.clone(), Value::Int(100));
    view.apply_kv_put(key2.clone(), Value::String("hello".into()));

    // Verify state
    assert_eq!(view.kv_count(), 2);
    assert_eq!(view.get_kv(&key1), Some(&Value::Int(100)));
    assert_eq!(view.get_kv(&key2), Some(&Value::String("hello".into())));
    assert!(view.contains_kv(&key1));
    assert!(view.contains_kv(&key2));

    // Update key1
    view.apply_kv_put(key1.clone(), Value::Int(200));
    assert_eq!(view.get_kv(&key1), Some(&Value::Int(200)));

    // Delete key2
    view.apply_kv_delete(&key2);
    assert!(!view.contains_kv(&key2));
    assert_eq!(view.kv_count(), 1);
}

#[test]
fn test_read_only_view_events() {
    let run_id = RunId::new();
    let mut view = ReadOnlyView::new(run_id);

    view.append_event("UserCreated".into(), Value::String("alice".into()));
    view.append_event("UserLogin".into(), Value::String("alice".into()));
    view.append_event("ItemPurchased".into(), Value::Int(42));

    assert_eq!(view.event_count(), 3);

    let events = view.events();
    assert_eq!(events[0].0, "UserCreated");
    assert_eq!(events[1].0, "UserLogin");
    assert_eq!(events[2].0, "ItemPurchased");
}

#[test]
fn test_read_only_view_operation_count() {
    let run_id = RunId::new();
    let ns = test_namespace();
    let mut view = ReadOnlyView::new(run_id);

    let key = Key::new_kv(ns.clone(), "key");

    view.apply_kv_put(key.clone(), Value::Int(1));
    assert_eq!(view.operation_count(), 1);

    view.apply_kv_put(key.clone(), Value::Int(2));
    assert_eq!(view.operation_count(), 2);

    view.apply_kv_delete(&key);
    assert_eq!(view.operation_count(), 3);

    view.append_event("Test".into(), Value::Null);
    assert_eq!(view.operation_count(), 4);
}

// ============================================================================
// diff_runs() Key-Level Comparison
// ============================================================================

#[test]
fn test_diff_views_identical() {
    let ns = test_namespace();
    let run_a = RunId::new();
    let run_b = RunId::new();

    let mut view_a = ReadOnlyView::new(run_a);
    let mut view_b = ReadOnlyView::new(run_b);

    // Same content
    view_a.apply_kv_put(Key::new_kv(ns.clone(), "key1"), Value::Int(1));
    view_b.apply_kv_put(Key::new_kv(ns.clone(), "key1"), Value::Int(1));

    let diff = diff_views(&view_a, &view_b);

    assert!(diff.is_empty());
    assert_eq!(diff.total_changes(), 0);
}

#[test]
fn test_diff_views_additions() {
    let ns = test_namespace();
    let run_a = RunId::new();
    let run_b = RunId::new();

    let mut view_a = ReadOnlyView::new(run_a);
    let mut view_b = ReadOnlyView::new(run_b);

    // B has more keys than A
    view_a.apply_kv_put(Key::new_kv(ns.clone(), "common"), Value::Int(1));
    view_b.apply_kv_put(Key::new_kv(ns.clone(), "common"), Value::Int(1));
    view_b.apply_kv_put(Key::new_kv(ns.clone(), "new_key"), Value::Int(2));

    let diff = diff_views(&view_a, &view_b);

    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.removed.len(), 0);
    assert_eq!(diff.modified.len(), 0);
    assert_eq!(diff.added[0].key, "new_key");
}

#[test]
fn test_diff_views_removals() {
    let ns = test_namespace();
    let run_a = RunId::new();
    let run_b = RunId::new();

    let mut view_a = ReadOnlyView::new(run_a);
    let mut view_b = ReadOnlyView::new(run_b);

    // A has more keys than B
    view_a.apply_kv_put(Key::new_kv(ns.clone(), "common"), Value::Int(1));
    view_a.apply_kv_put(Key::new_kv(ns.clone(), "old_key"), Value::Int(2));
    view_b.apply_kv_put(Key::new_kv(ns.clone(), "common"), Value::Int(1));

    let diff = diff_views(&view_a, &view_b);

    assert_eq!(diff.added.len(), 0);
    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.modified.len(), 0);
    assert_eq!(diff.removed[0].key, "old_key");
}

#[test]
fn test_diff_views_modifications() {
    let ns = test_namespace();
    let run_a = RunId::new();
    let run_b = RunId::new();

    let key = Key::new_kv(ns.clone(), "shared");

    let mut view_a = ReadOnlyView::new(run_a);
    let mut view_b = ReadOnlyView::new(run_b);

    view_a.apply_kv_put(key.clone(), Value::Int(1));
    view_b.apply_kv_put(key.clone(), Value::Int(2));

    let diff = diff_views(&view_a, &view_b);

    assert_eq!(diff.added.len(), 0);
    assert_eq!(diff.removed.len(), 0);
    assert_eq!(diff.modified.len(), 1);
    assert_eq!(diff.modified[0].key, "shared");
}

#[test]
fn test_diff_views_mixed_changes() {
    let ns = test_namespace();
    let run_a = RunId::new();
    let run_b = RunId::new();

    let mut view_a = ReadOnlyView::new(run_a);
    let mut view_b = ReadOnlyView::new(run_b);

    // A's keys
    view_a.apply_kv_put(Key::new_kv(ns.clone(), "only_a"), Value::Int(1));
    view_a.apply_kv_put(Key::new_kv(ns.clone(), "common"), Value::Int(2));
    view_a.apply_kv_put(Key::new_kv(ns.clone(), "modified"), Value::Int(3));

    // B's keys
    view_b.apply_kv_put(Key::new_kv(ns.clone(), "only_b"), Value::Int(10));
    view_b.apply_kv_put(Key::new_kv(ns.clone(), "common"), Value::Int(2));
    view_b.apply_kv_put(Key::new_kv(ns.clone(), "modified"), Value::Int(30));

    let diff = diff_views(&view_a, &view_b);

    assert_eq!(diff.added.len(), 1); // only_b
    assert_eq!(diff.removed.len(), 1); // only_a
    assert_eq!(diff.modified.len(), 1); // modified
    assert_eq!(diff.total_changes(), 3);
}

#[test]
fn test_diff_summary() {
    let diff = RunDiff {
        run_a: RunId::new(),
        run_b: RunId::new(),
        added: vec![
            DiffEntry::added("a".into(), PrimitiveType::Kv, "1".into()),
            DiffEntry::added("b".into(), PrimitiveType::Kv, "2".into()),
        ],
        removed: vec![DiffEntry::removed(
            "c".into(),
            PrimitiveType::Kv,
            "3".into(),
        )],
        modified: vec![
            DiffEntry::modified("d".into(), PrimitiveType::Kv, "4".into(), "5".into()),
            DiffEntry::modified("e".into(), PrimitiveType::Kv, "6".into(), "7".into()),
            DiffEntry::modified("f".into(), PrimitiveType::Kv, "8".into(), "9".into()),
        ],
    };

    assert_eq!(diff.summary(), "+2 -1 ~3 (total: 6)");
}

// ============================================================================
// Orphaned Run Detection
// ============================================================================

#[test]
fn test_orphaned_run_detection_basic() {
    let mut index = RunIndex::new();

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Create two active runs
    index.insert(run1, RunMetadata::new(run1, 1000, 0));
    index.insert(run2, RunMetadata::new(run2, 2000, 100));

    // Find active runs (potential orphans after crash)
    let active = index.find_active();
    assert_eq!(active.len(), 2);

    // Mark them as orphaned
    index.mark_orphaned(&active);

    assert_eq!(index.status(run1), RunStatus::Orphaned);
    assert_eq!(index.status(run2), RunStatus::Orphaned);
}

#[test]
fn test_orphaned_run_detection_mixed_states() {
    let mut index = RunIndex::new();

    let completed_run = RunId::new();
    let active_run1 = RunId::new();
    let active_run2 = RunId::new();

    // Create runs with different states
    let mut completed_meta = RunMetadata::new(completed_run, 1000, 0);
    completed_meta.complete(2000, 100);
    index.insert(completed_run, completed_meta);

    index.insert(active_run1, RunMetadata::new(active_run1, 3000, 200));
    index.insert(active_run2, RunMetadata::new(active_run2, 4000, 300));

    // Only active runs should be detected
    let active = index.find_active();
    assert_eq!(active.len(), 2);
    assert!(active.contains(&active_run1));
    assert!(active.contains(&active_run2));
    assert!(!active.contains(&completed_run));

    // Mark orphans
    index.mark_orphaned(&active);

    // Verify final states
    assert_eq!(index.status(completed_run), RunStatus::Completed);
    assert_eq!(index.status(active_run1), RunStatus::Orphaned);
    assert_eq!(index.status(active_run2), RunStatus::Orphaned);
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

    for _ in 0..1 {
        let run_id = RunId::new();
        let mut meta = RunMetadata::new(run_id, 1000, 0);
        meta.mark_orphaned();
        index.insert(run_id, meta);
    }

    let counts = index.count_by_status();
    assert_eq!(counts.get(&RunStatus::Active), Some(&3));
    assert_eq!(counts.get(&RunStatus::Completed), Some(&2));
    assert_eq!(counts.get(&RunStatus::Orphaned), Some(&1));
}

// ============================================================================
// Replay Invariants Tests
// ============================================================================

#[test]
fn test_replay_invariant_p5_deterministic() {
    // P5: Same inputs = Same view
    let run_id = RunId::new();
    let ns = test_namespace();

    // Create two views with the same operations
    let mut view1 = ReadOnlyView::new(run_id);
    let mut view2 = ReadOnlyView::new(run_id);

    let key1 = Key::new_kv(ns.clone(), "key1");
    let key2 = Key::new_kv(ns.clone(), "key2");

    // Apply same operations to both
    view1.apply_kv_put(key1.clone(), Value::Int(42));
    view1.apply_kv_put(key2.clone(), Value::String("hello".into()));
    view1.append_event("TestEvent".into(), Value::Null);

    view2.apply_kv_put(key1.clone(), Value::Int(42));
    view2.apply_kv_put(key2.clone(), Value::String("hello".into()));
    view2.append_event("TestEvent".into(), Value::Null);

    // Views should be identical
    assert_eq!(view1.kv_count(), view2.kv_count());
    assert_eq!(view1.event_count(), view2.event_count());
    assert_eq!(view1.get_kv(&key1), view2.get_kv(&key1));
    assert_eq!(view1.get_kv(&key2), view2.get_kv(&key2));

    // Diff should be empty
    let diff = diff_views(&view1, &view2);
    assert!(diff.is_empty());
}

#[test]
fn test_replay_invariant_p6_idempotent() {
    // P6: Running twice produces identical view
    let run_id = RunId::new();
    let ns = test_namespace();

    // Simulate replay by building the same view multiple times
    fn build_view(run_id: RunId, ns: Namespace) -> ReadOnlyView {
        let mut view = ReadOnlyView::new(run_id);
        view.apply_kv_put(Key::new_kv(ns.clone(), "counter"), Value::Int(1));
        view.apply_kv_put(Key::new_kv(ns.clone(), "counter"), Value::Int(2));
        view.apply_kv_put(Key::new_kv(ns.clone(), "counter"), Value::Int(3));
        view.append_event("Increment".into(), Value::Int(1));
        view.append_event("Increment".into(), Value::Int(2));
        view.append_event("Increment".into(), Value::Int(3));
        view
    }

    let view1 = build_view(run_id, ns.clone());
    let view2 = build_view(run_id, ns.clone());

    // Views should be identical
    let diff = diff_views(&view1, &view2);
    assert!(diff.is_empty());
}
