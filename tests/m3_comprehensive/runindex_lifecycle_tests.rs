//! Tier 1: RunIndex Lifecycle Tests (M3.16-M3.20)
//!
//! These tests verify RunIndex invariants around status transitions,
//! resurrection prevention, and cascading delete.
//!
//! ## Invariants Tested
//!
//! - M3.16: Valid Status Transitions - Only valid transitions allowed
//! - M3.17: No Resurrection - Cannot go from terminal to Active
//! - M3.18: Archived is Terminal - No transitions from Archived
//! - M3.19: Cascading Delete - delete_run() removes all primitive data
//! - M3.20: Status Updates Are Transactional - Atomic with other operations

use super::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::{RunIndex, RunStatus};

// ============================================================================
// M3.16: Valid Status Transitions
// ============================================================================
// Active -> Completed/Failed/Cancelled/Paused/Archived (valid)
// Paused -> Active/Cancelled/Archived (valid)
// Terminal -> Archived only (valid)
//
// What breaks if this fails?
// Invalid state machine. Completed run transitions to Active.

mod valid_status_transitions {
    use super::*;

    #[test]
    fn test_active_to_completed() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();

        let meta = tp.run_index.complete_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Completed);
    }

    #[test]
    fn test_active_to_failed() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();

        let meta = tp.run_index.fail_run("test-run", "error message").unwrap();
        assert_eq!(meta.value.status, RunStatus::Failed);
        assert_eq!(meta.value.error, Some("error message".to_string()));
    }

    #[test]
    fn test_active_to_cancelled() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();

        let meta = tp.run_index.cancel_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Cancelled);
    }

    #[test]
    fn test_active_to_paused() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();

        let meta = tp.run_index.pause_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Paused);
    }

    #[test]
    fn test_active_to_archived() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();

        let meta = tp.run_index.archive_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Archived);
    }

    #[test]
    fn test_paused_to_active() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.pause_run("test-run").unwrap();

        let meta = tp.run_index.resume_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Active);
    }

    #[test]
    fn test_paused_to_cancelled() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.pause_run("test-run").unwrap();

        let meta = tp.run_index.cancel_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Cancelled);
    }

    #[test]
    fn test_paused_to_archived() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.pause_run("test-run").unwrap();

        let meta = tp.run_index.archive_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Archived);
    }

    #[test]
    fn test_completed_to_archived() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.complete_run("test-run").unwrap();

        let meta = tp.run_index.archive_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Archived);
    }

    #[test]
    fn test_failed_to_archived() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.fail_run("test-run", "error").unwrap();

        let meta = tp.run_index.archive_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Archived);
    }

    #[test]
    fn test_cancelled_to_archived() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.cancel_run("test-run").unwrap();

        let meta = tp.run_index.archive_run("test-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Archived);
    }

    #[test]
    fn test_full_lifecycle_active_pause_resume_complete_archive() {
        let tp = TestPrimitives::new();

        // Create (Active)
        let meta = tp.run_index.create_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Active);

        // Pause
        let meta = tp.run_index.pause_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Paused);

        // Resume
        let meta = tp.run_index.resume_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Active);

        // Complete
        let meta = tp.run_index.complete_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Completed);

        // Archive
        let meta = tp.run_index.archive_run("lifecycle-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Archived);
    }
}

// ============================================================================
// M3.17: No Resurrection
// ============================================================================
// Completed -> Active (error)
// Failed -> Active (error)
// Cancelled -> Active (error)
//
// What breaks if this fails?
// Zombie runs. Completed runs restart. Audit trails invalid.

mod no_resurrection {
    use super::*;

    #[test]
    fn test_completed_cannot_resurrect_to_active() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.complete_run("test-run").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Active);
        assert!(result.is_err(), "Completed -> Active should fail");
    }

    #[test]
    fn test_failed_cannot_resurrect_to_active() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.fail_run("test-run", "error").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Active);
        assert!(result.is_err(), "Failed -> Active should fail");
    }

    #[test]
    fn test_cancelled_cannot_resurrect_to_active() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.cancel_run("test-run").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Active);
        assert!(result.is_err(), "Cancelled -> Active should fail");
    }

    #[test]
    fn test_completed_cannot_go_to_paused() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.complete_run("test-run").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Paused);
        assert!(result.is_err(), "Completed -> Paused should fail");
    }

    #[test]
    fn test_failed_cannot_go_to_completed() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.fail_run("test-run", "error").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Completed);
        assert!(result.is_err(), "Failed -> Completed should fail");
    }

    #[test]
    fn test_paused_cannot_go_to_completed_directly() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.pause_run("test-run").unwrap();

        // Paused can only go to Active, Cancelled, or Archived
        let result = tp.run_index.update_status("test-run", RunStatus::Completed);
        assert!(result.is_err(), "Paused -> Completed should fail");
    }
}

// ============================================================================
// M3.18: Archived is Terminal
// ============================================================================
// Archived -> any other status (error)
// Data still accessible after archive
// Archive is soft delete
//
// What breaks if this fails?
// Archived runs revive. "Deleted" data comes back.

mod archived_is_terminal {
    use super::*;

    #[test]
    fn test_archived_cannot_go_to_active() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.archive_run("test-run").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Active);
        assert!(result.is_err(), "Archived -> Active should fail");
    }

    #[test]
    fn test_archived_cannot_go_to_completed() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.archive_run("test-run").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Completed);
        assert!(result.is_err(), "Archived -> Completed should fail");
    }

    #[test]
    fn test_archived_cannot_go_to_failed() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.archive_run("test-run").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Failed);
        assert!(result.is_err(), "Archived -> Failed should fail");
    }

    #[test]
    fn test_archived_cannot_go_to_paused() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.archive_run("test-run").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Paused);
        assert!(result.is_err(), "Archived -> Paused should fail");
    }

    #[test]
    fn test_archived_cannot_go_to_cancelled() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.archive_run("test-run").unwrap();

        let result = tp.run_index.update_status("test-run", RunStatus::Cancelled);
        assert!(result.is_err(), "Archived -> Cancelled should fail");
    }

    #[test]
    fn test_archived_cannot_re_archive() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.archive_run("test-run").unwrap();

        // Even re-archiving should fail (it's terminal)
        let result = tp.run_index.update_status("test-run", RunStatus::Archived);
        assert!(result.is_err(), "Archived -> Archived should fail");
    }

    #[test]
    fn test_archived_data_still_accessible() {
        let tp = TestPrimitives::new();
        let meta = tp.run_index.create_run("test-run").unwrap();
        let run_id = RunId::from_string(&meta.value.run_id).unwrap();

        // Write some data
        tp.kv.put(&run_id, "key", values::int(42)).unwrap();
        tp.event_log
            .append(&run_id, "event", values::null())
            .unwrap();

        // Archive
        tp.run_index.archive_run("test-run").unwrap();

        // Data should still be accessible (soft delete)
        assert_eq!(tp.kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(values::int(42)));
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 1);
    }

    #[test]
    fn test_archive_is_soft_delete() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        tp.run_index.archive_run("test-run").unwrap();

        // Run metadata still exists (soft delete)
        let meta = tp.run_index.get_run("test-run").unwrap();
        assert!(meta.is_some());
        assert_eq!(meta.unwrap().value.status, RunStatus::Archived);
    }
}

// ============================================================================
// M3.19: Cascading Delete
// ============================================================================
// delete_run() removes all primitive data
// KV, Events, States all deleted
// Other runs unaffected
//
// What breaks if this fails?
// Orphaned data. Deleted run leaves behind KV entries, events.

mod cascading_delete {
    use super::*;

    #[test]
    fn test_delete_run_removes_metadata() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("test-run").unwrap();
        assert!(tp.run_index.exists("test-run").unwrap());

        tp.run_index.delete_run("test-run").unwrap();
        assert!(!tp.run_index.exists("test-run").unwrap());
    }

    #[test]
    fn test_delete_run_removes_kv_data() {
        let tp = TestPrimitives::new();
        let meta = tp.run_index.create_run("test-run").unwrap();
        let run_id = RunId::from_string(&meta.value.run_id).unwrap();

        // Write KV data
        tp.kv.put(&run_id, "key1", values::int(1)).unwrap();
        tp.kv.put(&run_id, "key2", values::int(2)).unwrap();
        tp.kv.put(&run_id, "key3", values::int(3)).unwrap();

        // Delete run
        tp.run_index.delete_run("test-run").unwrap();

        // All KV data gone
        assert!(tp.kv.get(&run_id, "key1").unwrap().is_none());
        assert!(tp.kv.get(&run_id, "key2").unwrap().is_none());
        assert!(tp.kv.get(&run_id, "key3").unwrap().is_none());
    }

    #[test]
    fn test_delete_run_removes_events() {
        let tp = TestPrimitives::new();
        let meta = tp.run_index.create_run("test-run").unwrap();
        let run_id = RunId::from_string(&meta.value.run_id).unwrap();

        // Append events
        for i in 0..10 {
            tp.event_log
                .append(&run_id, "event", values::int(i))
                .unwrap();
        }
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 10);

        // Delete run
        tp.run_index.delete_run("test-run").unwrap();

        // All events gone
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 0);
    }

    #[test]
    fn test_delete_run_removes_states() {
        let tp = TestPrimitives::new();
        let meta = tp.run_index.create_run("test-run").unwrap();
        let run_id = RunId::from_string(&meta.value.run_id).unwrap();

        // Create state cells
        tp.state_cell
            .init(&run_id, "cell1", values::int(1))
            .unwrap();
        tp.state_cell
            .init(&run_id, "cell2", values::int(2))
            .unwrap();
        assert!(tp.state_cell.exists(&run_id, "cell1").unwrap());
        assert!(tp.state_cell.exists(&run_id, "cell2").unwrap());

        // Delete run
        tp.run_index.delete_run("test-run").unwrap();

        // All state cells gone
        assert!(!tp.state_cell.exists(&run_id, "cell1").unwrap());
        assert!(!tp.state_cell.exists(&run_id, "cell2").unwrap());
    }

    #[test]
    fn test_delete_run_removes_all_primitive_data() {
        let tp = TestPrimitives::new();
        let meta = tp.run_index.create_run("test-run").unwrap();
        let run_id = RunId::from_string(&meta.value.run_id).unwrap();

        // Write to primitives
        tp.kv.put(&run_id, "key", values::int(1)).unwrap();
        tp.event_log
            .append(&run_id, "event", values::null())
            .unwrap();
        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        // Verify data exists
        assert!(tp.kv.get(&run_id, "key").unwrap().is_some());
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 1);
        assert!(tp.state_cell.exists(&run_id, "cell").unwrap());

        // Delete run (cascading)
        tp.run_index.delete_run("test-run").unwrap();

        // ALL data gone
        assert!(tp.kv.get(&run_id, "key").unwrap().is_none());
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 0);
        assert!(!tp.state_cell.exists(&run_id, "cell").unwrap());
    }

    #[test]
    fn test_delete_run_does_not_affect_other_runs() {
        let tp = TestPrimitives::new();

        // Create two runs
        let meta1 = tp.run_index.create_run("run-1").unwrap();
        let meta2 = tp.run_index.create_run("run-2").unwrap();
        let run_id1 = RunId::from_string(&meta1.value.run_id).unwrap();
        let run_id2 = RunId::from_string(&meta2.value.run_id).unwrap();

        // Write to both
        tp.kv.put(&run_id1, "key", values::string("run1")).unwrap();
        tp.kv.put(&run_id2, "key", values::string("run2")).unwrap();
        tp.event_log
            .append(&run_id1, "event", values::null())
            .unwrap();
        tp.event_log
            .append(&run_id2, "event", values::null())
            .unwrap();

        // Delete run-1
        tp.run_index.delete_run("run-1").unwrap();

        // run-1 data gone
        assert!(tp.kv.get(&run_id1, "key").unwrap().is_none());
        assert_eq!(tp.event_log.len(&run_id1).unwrap(), 0);

        // run-2 data intact
        assert_eq!(
            tp.kv.get(&run_id2, "key").unwrap().map(|v| v.value),
            Some(values::string("run2"))
        );
        assert_eq!(tp.event_log.len(&run_id2).unwrap(), 1);
        assert!(tp.run_index.exists("run-2").unwrap());
    }

    #[test]
    fn test_delete_nonexistent_run_fails() {
        let tp = TestPrimitives::new();

        let result = tp.run_index.delete_run("nonexistent");
        assert!(result.is_err(), "Deleting nonexistent run should fail");
    }
}

// ============================================================================
// M3.20: Status Updates Are Transactional
// ============================================================================
// RunIndex status updates are WAL-backed
// Status changes are atomic with other operations in same transaction
// Recovery preserves last committed status
//
// What breaks if this fails?
// Status/data inconsistency. Run shows "Completed" but data is partial.

mod status_updates_transactional {
    use super::*;

    #[test]
    fn test_status_update_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();

        let run_name = unique_key("run");

        // Create and transition run
        {
            let p = ptp.open_strict();
            p.run_index.create_run(&run_name).unwrap();
            p.run_index.complete_run(&run_name).unwrap();
        }

        // Recover and verify status preserved
        {
            let p = ptp.open();
            let meta = p.run_index.get_run(&run_name).unwrap().unwrap();
            assert_eq!(
                meta.value.status,
                RunStatus::Completed,
                "Status not preserved after recovery"
            );
        }
    }

    #[test]
    fn test_completed_at_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();

        let run_name = unique_key("run");
        let completed_at;

        {
            let p = ptp.open_strict();
            p.run_index.create_run(&run_name).unwrap();
            let meta = p.run_index.complete_run(&run_name).unwrap();
            completed_at = meta.value.completed_at;
        }

        {
            let p = ptp.open();
            let meta = p.run_index.get_run(&run_name).unwrap().unwrap();
            assert_eq!(
                meta.value.completed_at, completed_at,
                "completed_at not preserved"
            );
        }
    }

    #[test]
    fn test_error_message_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();

        let run_name = unique_key("run");
        let error_msg = "Something went wrong during processing";

        {
            let p = ptp.open_strict();
            p.run_index.create_run(&run_name).unwrap();
            p.run_index.fail_run(&run_name, error_msg).unwrap();
        }

        {
            let p = ptp.open();
            let meta = p.run_index.get_run(&run_name).unwrap().unwrap();
            assert_eq!(
                meta.value.error,
                Some(error_msg.to_string()),
                "Error message not preserved"
            );
        }
    }

    #[test]
    fn test_tags_survive_recovery() {
        let ptp = PersistentTestPrimitives::new();

        let run_name = unique_key("run");
        let tags = vec!["experiment".to_string(), "v2".to_string()];

        {
            let p = ptp.open_strict();
            p.run_index
                .create_run_with_options(&run_name, None, tags.clone(), values::null())
                .unwrap();
        }

        {
            let p = ptp.open();
            let meta = p.run_index.get_run(&run_name).unwrap().unwrap();
            assert_eq!(meta.value.tags, tags, "Tags not preserved after recovery");
        }
    }

    #[test]
    fn test_status_index_consistent_after_recovery() {
        let ptp = PersistentTestPrimitives::new();

        let run_name = unique_key("run");

        {
            let p = ptp.open_strict();
            p.run_index.create_run(&run_name).unwrap();
            p.run_index.complete_run(&run_name).unwrap();
        }

        {
            let p = ptp.open();

            // Query by status should return the run
            let completed = p.run_index.query_by_status(RunStatus::Completed).unwrap();
            assert!(
                completed.iter().any(|m| m.name == run_name),
                "Run not found in Completed index after recovery"
            );

            // Should NOT be in Active index
            let active = p.run_index.query_by_status(RunStatus::Active).unwrap();
            assert!(
                !active.iter().any(|m| m.name == run_name),
                "Run still in Active index after transition"
            );
        }
    }

    #[test]
    fn test_multiple_transitions_survive_recovery() {
        let ptp = PersistentTestPrimitives::new();

        let run_name = unique_key("run");

        {
            let p = ptp.open_strict();
            p.run_index.create_run(&run_name).unwrap();
            p.run_index.pause_run(&run_name).unwrap();
            p.run_index.resume_run(&run_name).unwrap();
            p.run_index.complete_run(&run_name).unwrap();
            p.run_index.archive_run(&run_name).unwrap();
        }

        {
            let p = ptp.open();
            let meta = p.run_index.get_run(&run_name).unwrap().unwrap();
            assert_eq!(
                meta.value.status,
                RunStatus::Archived,
                "Final status not preserved"
            );
        }
    }
}
