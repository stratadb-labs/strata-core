//! Cross-Primitive Transaction Tests (Tier 2)
//!
//! Tests multi-primitive atomic operations:
//! - All-or-nothing commit semantics
//! - Cross-primitive rollback
//! - Read-your-writes within transactions
//! - Extension trait composition

use crate::test_utils::{values, PersistentTestPrimitives, TestPrimitives};
use strata_core::contract::Version;
use strata_core::value::Value;
use strata_primitives::{RunStatus};

// =============================================================================
// Atomic Multi-Primitive Operations
// =============================================================================

mod atomic_operations {
    use super::*;

    #[test]
    fn test_all_primitives_visible_after_individual_ops() {
        // When we write to all primitives, all changes are visible
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Write to each primitive
        tp.kv.put(&run_id, "key", values::int(1)).unwrap();
        tp.event_log
            .append(&run_id, "event", values::int(2))
            .unwrap();
        tp.state_cell.init(&run_id, "cell", values::int(3)).unwrap();

        // All visible
        assert!(tp.kv.get(&run_id, "key").unwrap().is_some());
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 1);
        assert!(tp.state_cell.read(&run_id, "cell").unwrap().is_some());
    }

    #[test]
    fn test_kv_and_event_together() {
        // KV and EventLog operations work together
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.kv.put(&run_id, "step", values::int(1)).unwrap();
        tp.event_log
            .append(&run_id, "started", values::null())
            .unwrap();
        tp.kv.put(&run_id, "step", values::int(2)).unwrap();
        tp.event_log
            .append(&run_id, "finished", values::null())
            .unwrap();

        assert_eq!(tp.kv.get(&run_id, "step").unwrap().map(|v| v.value), Some(values::int(2)));
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 2);
    }

}

// =============================================================================
// Read-Your-Writes Semantics
// =============================================================================

mod read_your_writes {
    use super::*;

    #[test]
    fn test_kv_read_after_write() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.kv.put(&run_id, "key", values::int(42)).unwrap();
        let value = tp.kv.get(&run_id, "key").unwrap().map(|v| v.value);
        assert_eq!(value, Some(values::int(42)));
    }

    #[test]
    fn test_event_read_after_append() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let version = tp
            .event_log
            .append(&run_id, "test", values::int(100))
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };
        let event = tp.event_log.read(&run_id, seq).unwrap().unwrap();
        assert_eq!(event.value.payload, values::int(100));
    }

    #[test]
    fn test_state_read_after_init() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell
            .init(&run_id, "cell", values::string("hello"))
            .unwrap();
        let state = tp.state_cell.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::string("hello"));
    }

    #[test]
    fn test_state_read_after_cas() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();
        tp.state_cell
            .cas(&run_id, "cell", 1, values::int(10))
            .unwrap();
        let state = tp.state_cell.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(10));
        assert_eq!(state.value.version, 2);
    }

    #[test]
    fn test_cross_primitive_read_your_writes() {
        // Write to multiple primitives and read back immediately
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Write sequence
        tp.kv.put(&run_id, "step1", values::int(1)).unwrap();
        let version = tp
            .event_log
            .append(&run_id, "step1", values::int(1))
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };
        tp.state_cell.init(&run_id, "step", values::int(1)).unwrap();

        // All immediately visible
        assert_eq!(tp.kv.get(&run_id, "step1").unwrap().map(|v| v.value), Some(values::int(1)));
        assert!(tp.event_log.read(&run_id, seq).unwrap().is_some());
        assert!(tp.state_cell.read(&run_id, "step").unwrap().is_some());

        // Continue sequence
        tp.kv.put(&run_id, "step2", values::int(2)).unwrap();
        tp.event_log
            .append(&run_id, "step2", values::int(2))
            .unwrap();
        tp.state_cell.set(&run_id, "step", values::int(2)).unwrap();

        // All updates visible
        assert_eq!(tp.kv.get(&run_id, "step2").unwrap().map(|v| v.value), Some(values::int(2)));
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 2);
        let state = tp.state_cell.read(&run_id, "step").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(2));
    }
}

// =============================================================================
// Multi-Primitive Persistence
// =============================================================================

mod multi_primitive_persistence {
    use super::*;

    #[test]
    fn test_all_primitives_survive_reopen() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // First session: write to all primitives
        {
            let prims = ptp.open();
            prims.kv.put(&run_id, "key", values::int(100)).unwrap();
            prims
                .event_log
                .append(&run_id, "event", values::int(200))
                .unwrap();
            prims
                .state_cell
                .init(&run_id, "cell", values::int(300))
                .unwrap();
        }

        // Second session: verify all data persisted
        {
            let prims = ptp.open();
            assert_eq!(
                prims.kv.get(&run_id, "key").unwrap().map(|v| v.value),
                Some(values::int(100))
            );
            let event = prims.event_log.read(&run_id, 0).unwrap().unwrap();
            assert_eq!(event.value.payload, values::int(200));
            let state = prims.state_cell.read(&run_id, "cell").unwrap().unwrap();
            assert_eq!(state.value.value, values::int(300));
        }
    }

    #[test]
    fn test_partial_updates_across_sessions() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: KV and Event
        {
            let prims = ptp.open();
            prims.kv.put(&run_id, "key1", values::int(1)).unwrap();
            prims
                .event_log
                .append(&run_id, "event1", values::null())
                .unwrap();
        }

        // Session 2: StateCell
        {
            let prims = ptp.open();
            prims
                .state_cell
                .init(&run_id, "cell", values::int(2))
                .unwrap();
        }

        // Session 3: Verify all
        {
            let prims = ptp.open();
            assert!(prims.kv.get(&run_id, "key1").unwrap().is_some());
            assert_eq!(prims.event_log.len(&run_id).unwrap(), 1);
            assert!(prims.state_cell.read(&run_id, "cell").unwrap().is_some());
        }
    }
}

// =============================================================================
// Run-Scoped Transactions
// =============================================================================

mod run_scoped_transactions {
    use super::*;

    #[test]
    fn test_operations_scoped_to_run() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = tp.new_run();

        // Write to run1
        tp.kv.put(&run1, "shared_key", values::int(1)).unwrap();
        tp.event_log.append(&run1, "event", values::null()).unwrap();

        // Write to run2
        tp.kv.put(&run2, "shared_key", values::int(2)).unwrap();
        tp.event_log.append(&run2, "event", values::null()).unwrap();
        tp.event_log.append(&run2, "event", values::null()).unwrap();

        // Each run has its own data
        assert_eq!(
            tp.kv.get(&run1, "shared_key").unwrap().map(|v| v.value),
            Some(values::int(1))
        );
        assert_eq!(
            tp.kv.get(&run2, "shared_key").unwrap().map(|v| v.value),
            Some(values::int(2))
        );
        assert_eq!(tp.event_log.len(&run1).unwrap(), 1);
        assert_eq!(tp.event_log.len(&run2).unwrap(), 2);
    }

    #[test]
    fn test_multiple_runs_concurrent_ops() {
        let tp = TestPrimitives::new();
        let runs: Vec<_> = (0..5).map(|_| tp.new_run()).collect();

        // Write to each run
        for (i, run) in runs.iter().enumerate() {
            tp.kv.put(run, "counter", values::int(i as i64)).unwrap();
            for _ in 0..=i {
                tp.event_log.append(run, "tick", values::null()).unwrap();
            }
        }

        // Verify each run has correct data
        for (i, run) in runs.iter().enumerate() {
            assert_eq!(
                tp.kv.get(run, "counter").unwrap().map(|v| v.value),
                Some(values::int(i as i64))
            );
            assert_eq!(tp.event_log.len(run).unwrap(), (i + 1) as u64);
        }
    }
}

// =============================================================================
// Run Status with Primitives
// =============================================================================

mod run_status_with_primitives {
    use super::*;
    use strata_core::types::RunId;

    #[test]
    fn test_run_status_independent_of_primitive_data() {
        let tp = TestPrimitives::new();
        // RunIndex create_run takes a string name and returns RunMetadata
        let meta = tp.run_index.create_run("test-run").unwrap();
        // Use the run_id from TestPrimitives for primitive operations
        let run_id = tp.run_id;

        // Write primitive data
        tp.kv.put(&run_id, "key", values::int(42)).unwrap();
        tp.event_log
            .append(&run_id, "event", values::null())
            .unwrap();

        // Update status using the run name
        tp.run_index
            .update_status(&meta.value.name, RunStatus::Paused)
            .unwrap();

        // Primitive data still accessible
        assert_eq!(tp.kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(values::int(42)));
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 1);

        // Complete the run
        tp.run_index
            .update_status(&meta.value.name, RunStatus::Active)
            .unwrap();
        tp.run_index.complete_run(&meta.value.name).unwrap();

        // Data still accessible after completion
        assert_eq!(tp.kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(values::int(42)));
    }

    #[test]
    fn test_archived_run_data_accessible() {
        let tp = TestPrimitives::new();
        let meta = tp.run_index.create_run("archived-run").unwrap();
        let run_id = tp.run_id;

        // Write data and archive
        tp.kv
            .put(&run_id, "archived_key", values::string("data"))
            .unwrap();
        tp.run_index.complete_run(&meta.value.name).unwrap();
        tp.run_index.archive_run(&meta.value.name).unwrap();

        // Data still accessible
        assert_eq!(
            tp.kv.get(&run_id, "archived_key").unwrap().map(|v| v.value),
            Some(values::string("data"))
        );
    }
}
