//! Tier 7: Run Lifecycle Tests
//!
//! Tests for begin_run, end_run, orphan detection.

use crate::test_utils::*;
use strata_core::run_types::{RunMetadata, RunStatus};
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::RunIndex;
use strata_primitives::KVStore;

/// Run ID is unique
#[test]
fn test_run_id_unique() {
    let ids: Vec<RunId> = (0..1000).map(|_| RunId::new()).collect();

    // All IDs should be unique
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(ids[i], ids[j], "RunId collision at {} and {}", i, j);
        }
    }
}

/// RunStatus transitions
#[test]
fn test_run_status_transitions() {
    let run_id = RunId::new();
    let mut meta = RunMetadata::new(run_id, 1000, 0);

    // Initial state is Active
    assert_eq!(meta.status, RunStatus::Active);
    assert!(meta.status.is_active());
    assert!(meta.status.exists());

    // Transition to Completed
    meta.complete(2000, 100);
    assert_eq!(meta.status, RunStatus::Completed);
    assert!(meta.status.is_completed());
    assert!(meta.status.exists());
}

/// RunStatus orphaned transition
#[test]
fn test_run_status_orphaned() {
    let run_id = RunId::new();
    let mut meta = RunMetadata::new(run_id, 1000, 0);

    // Active
    assert!(meta.status.is_active());

    // Mark orphaned
    meta.mark_orphaned();
    assert_eq!(meta.status, RunStatus::Orphaned);
    assert!(meta.status.is_orphaned());
    assert!(meta.status.exists());
}

/// RunStatus not found
#[test]
fn test_run_status_not_found() {
    let status = RunStatus::NotFound;
    assert!(!status.exists());
    assert!(!status.is_active());
    assert!(!status.is_completed());
    assert!(!status.is_orphaned());
}

/// RunMetadata duration calculation
#[test]
fn test_run_metadata_duration() {
    let run_id = RunId::new();
    let mut meta = RunMetadata::new(run_id, 1000, 0);

    // No duration before completion
    assert_eq!(meta.duration_micros(), None);

    // Complete
    meta.complete(2500, 100);
    assert_eq!(meta.duration_micros(), Some(1500));
}

/// RunIndex basic operations
#[test]
fn test_run_index_basic() {
    let mut index = RunIndex::new();

    let run_id = RunId::new();
    let meta = RunMetadata::new(run_id, 1000, 0);

    // Insert
    index.insert(run_id, meta);
    assert!(index.exists(run_id));

    // Get
    let retrieved = index.get(run_id);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().run_id, run_id);
}

/// RunIndex find active
#[test]
fn test_run_index_find_active() {
    let mut index = RunIndex::new();

    let active_id = RunId::new();
    let completed_id = RunId::new();

    // Insert active run
    index.insert(active_id, RunMetadata::new(active_id, 1000, 0));

    // Insert completed run
    let mut completed_meta = RunMetadata::new(completed_id, 1000, 0);
    completed_meta.complete(2000, 100);
    index.insert(completed_id, completed_meta);

    // Find active
    let active = index.find_active();
    assert!(active.contains(&active_id));
    assert!(!active.contains(&completed_id));
}

/// RunIndex mark orphaned
#[test]
fn test_run_index_mark_orphaned() {
    let mut index = RunIndex::new();

    let run_id1 = RunId::new();
    let run_id2 = RunId::new();

    index.insert(run_id1, RunMetadata::new(run_id1, 1000, 0));
    index.insert(run_id2, RunMetadata::new(run_id2, 1000, 0));

    // Mark first as orphaned
    index.mark_orphaned(&[run_id1]);

    assert_eq!(index.status(run_id1), RunStatus::Orphaned);
    assert_eq!(index.status(run_id2), RunStatus::Active);
}

/// RunIndex event recording
#[test]
fn test_run_index_event_recording() {
    let mut index = RunIndex::new();

    let run_id = RunId::new();
    index.insert(run_id, RunMetadata::new(run_id, 1000, 0));

    // Record events
    index.record_event(run_id, 100);
    index.record_event(run_id, 200);
    index.record_event(run_id, 300);

    // Check event offsets
    let offsets = index.get_event_offsets(run_id);
    assert!(offsets.is_some());
    assert_eq!(offsets.unwrap(), &[100, 200, 300]);

    // Check event count in metadata
    let meta = index.get(run_id).unwrap();
    assert_eq!(meta.event_count, 3);
}

/// RunIndex list runs
#[test]
fn test_run_index_list_runs() {
    let mut index = RunIndex::new();

    let run_ids: Vec<RunId> = (0..5).map(|_| RunId::new()).collect();

    for run_id in &run_ids {
        index.insert(*run_id, RunMetadata::new(*run_id, 1000, 0));
    }

    let listed = index.list_run_ids();
    assert_eq!(listed.len(), 5);

    for run_id in &run_ids {
        assert!(listed.contains(run_id));
    }
}

/// RunIndex count by status
#[test]
fn test_run_index_count_by_status() {
    let mut index = RunIndex::new();

    // Add various runs
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

/// Run persists across restart
#[test]
fn test_run_persists_across_restart() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "run_data", Value::String("important".into()))
        .unwrap();

    test_db.reopen();

    // Run data should persist
    let kv = test_db.kv();
    let value = kv.get(&run_id, "run_data").unwrap();
    assert!(value.is_some());
}
