//! Substrate Invariant Tests (M3.21-M3.24)
//!
//! These tests verify the architectural invariants that cement the M3 philosophy:
//! - Primitives are projections over KV storage
//! - Cross-primitive ordering consistency
//! - Replay metadata contract for M5 forward compatibility
//! - No implicit coupling between primitives

use crate::test_utils::{values, PersistentTestPrimitives, TestPrimitives};
use strata_core::contract::Version;
use strata_primitives::{EventLog, KVStore, RunIndex, StateCell, TraceStore, TraceType};

// =============================================================================
// M3.21: Primitives Are Projections Over KV (Canonical Source)
// =============================================================================
//
// All M3 primitives ultimately store data as key-value pairs. This test proves
// that primitive state can be reconstructed from the underlying storage.
//
// What breaks if this fails?
// - Non-reconstructable state
// - If primitives have hidden state, recovery is impossible
// - Backup/restore breaks

mod primitives_are_projections {
    use super::*;

    #[test]
    fn test_eventlog_data_persists_via_storage() {
        // EventLog data is stored in the underlying database, not in-memory
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append events using the EventLog
        tp.event_log
            .append(&run_id, "event_type_1", values::string("payload_1"))
            .unwrap();
        tp.event_log
            .append(&run_id, "event_type_2", values::string("payload_2"))
            .unwrap();

        // Create a new EventLog facade pointing to same database
        let event_log_2 = EventLog::new(tp.db.clone());

        // The new facade sees the same data - proving data is in storage, not facade
        let events = event_log_2.read_range(&run_id, 0, 10).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].value.event_type, "event_type_1");
        assert_eq!(events[1].value.event_type, "event_type_2");
    }

    #[test]
    fn test_statecell_data_persists_via_storage() {
        // StateCell data is stored in the underlying database
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Initialize and update cells
        tp.state_cell
            .init(&run_id, "cell_1", values::int(100))
            .unwrap();
        tp.state_cell
            .set(&run_id, "cell_1", values::int(200))
            .unwrap();

        // Create a new StateCell facade
        let state_cell_2 = StateCell::new(tp.db.clone());

        // New facade sees same data
        let state = state_cell_2.read(&run_id, "cell_1").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(200));
        assert!(state.value.version >= 1);
    }

    #[test]
    fn test_tracestore_data_persists_via_storage() {
        // TraceStore data is stored in the underlying database
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Record traces
        let trace_id = tp
            .trace_store
            .record(
                &run_id,
                TraceType::Thought {
                    content: "thinking...".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap()
            .value;

        // Create new TraceStore facade
        let trace_store_2 = TraceStore::new(tp.db.clone());

        // New facade can retrieve the trace
        let trace = trace_store_2.get(&run_id, &trace_id).unwrap().unwrap();
        assert!(matches!(trace.value.trace_type, TraceType::Thought { .. }));
    }

    #[test]
    fn test_kv_data_persists_via_storage() {
        // KVStore data is stored in the underlying database
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Put values
        tp.kv.put(&run_id, "key_1", values::int(42)).unwrap();
        tp.kv
            .put(&run_id, "key_2", values::string("hello"))
            .unwrap();

        // Create new KVStore facade
        let kv_2 = KVStore::new(tp.db.clone());

        // New facade sees same data
        assert_eq!(kv_2.get(&run_id, "key_1").unwrap().map(|v| v.value), Some(values::int(42)));
        assert_eq!(
            kv_2.get(&run_id, "key_2").unwrap().map(|v| v.value),
            Some(values::string("hello"))
        );
    }

    #[test]
    fn test_runindex_data_persists_via_storage() {
        // RunIndex data is stored in the underlying database
        let tp = TestPrimitives::new();

        // Create a run
        let meta = tp.run_index.create_run("test-run").unwrap();

        // Create new RunIndex facade
        let run_index_2 = RunIndex::new(tp.db.clone());

        // New facade can see the run
        let run_info = run_index_2.get_run(&meta.value.name).unwrap();
        assert!(run_info.is_some());
    }

    #[test]
    fn test_data_survives_all_facades_dropped() {
        // Even if all primitive facades are dropped, data persists
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // First session: write data and drop everything
        {
            let prims = ptp.open();
            prims
                .kv
                .put(&run_id, "persistent_key", values::int(999))
                .unwrap();
            prims
                .event_log
                .append(&run_id, "persistent_event", values::null())
                .unwrap();
            prims
                .state_cell
                .init(&run_id, "persistent_cell", values::bool_val(true))
                .unwrap();
            // All facades dropped here
        }

        // Second session: reopen and verify
        {
            let prims = ptp.open();
            assert_eq!(
                prims.kv.get(&run_id, "persistent_key").unwrap().map(|v| v.value),
                Some(values::int(999))
            );
            assert_eq!(prims.event_log.len(&run_id).unwrap(), 1);
            assert!(prims
                .state_cell
                .read(&run_id, "persistent_cell")
                .unwrap()
                .is_some());
        }
    }
}

// =============================================================================
// M3.22: Cross-Primitive Ordering Consistency
// =============================================================================
//
// Operations within a single transaction form a consistent snapshot.
// All primitive operations in a transaction are visible atomically (all-or-nothing).
//
// Note: We do NOT guarantee real-time ordering across transactions.
//
// What breaks if this fails?
// - Partial visibility
// - Some primitive operations visible, others not
// - Inconsistent cross-primitive reads within one transaction

mod cross_primitive_ordering {
    use super::*;

    #[test]
    fn test_all_primitive_writes_visible_after_commit() {
        // When a transaction commits, ALL primitive changes are visible
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Write to multiple primitives
        tp.kv.put(&run_id, "key", values::int(1)).unwrap();
        tp.event_log
            .append(&run_id, "event", values::int(2))
            .unwrap();
        tp.state_cell.init(&run_id, "cell", values::int(3)).unwrap();
        tp.trace_store
            .record(
                &run_id,
                TraceType::Custom {
                    name: "trace".into(),
                    data: values::int(4),
                },
                vec![],
                values::null(),
            )
            .unwrap();

        // All are visible
        assert!(tp.kv.get(&run_id, "key").unwrap().is_some());
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 1);
        assert!(tp.state_cell.read(&run_id, "cell").unwrap().is_some());
        assert!(tp.trace_store.count(&run_id).unwrap() >= 1);
    }

    #[test]
    fn test_cross_primitive_snapshot_within_reads() {
        // Multiple reads within a session see consistent state
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Setup: write initial data
        tp.kv.put(&run_id, "counter", values::int(10)).unwrap();
        tp.event_log
            .append(&run_id, "init", values::int(10))
            .unwrap();
        tp.state_cell
            .init(&run_id, "state", values::int(10))
            .unwrap();

        // Read all primitives
        let kv_val = tp.kv.get(&run_id, "counter").unwrap().map(|v| v.value);
        let events = tp.event_log.read_range(&run_id, 0, 100).unwrap();
        let state = tp.state_cell.read(&run_id, "state").unwrap();

        // All reflect the same logical state
        assert_eq!(kv_val, Some(values::int(10)));
        assert_eq!(events.len(), 1);
        assert!(state.is_some());
    }

    #[test]
    fn test_ordering_within_same_primitive() {
        // Operations on same primitive within a session are ordered
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Sequential writes
        tp.kv.put(&run_id, "key", values::int(1)).unwrap();
        tp.kv.put(&run_id, "key", values::int(2)).unwrap();
        tp.kv.put(&run_id, "key", values::int(3)).unwrap();

        // Last write wins
        assert_eq!(tp.kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(values::int(3)));
    }

    #[test]
    fn test_eventlog_preserves_append_order() {
        // Events are ordered by sequence, reflecting append order
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.event_log
            .append(&run_id, "first", values::int(1))
            .unwrap();
        tp.event_log
            .append(&run_id, "second", values::int(2))
            .unwrap();
        tp.event_log
            .append(&run_id, "third", values::int(3))
            .unwrap();

        let events = tp.event_log.read_range(&run_id, 0, 10).unwrap();
        assert_eq!(events[0].value.event_type, "first");
        assert_eq!(events[1].value.event_type, "second");
        assert_eq!(events[2].value.event_type, "third");
        assert!(events[0].value.sequence < events[1].value.sequence);
        assert!(events[1].value.sequence < events[2].value.sequence);
    }
}

// =============================================================================
// M3.23: Replay Metadata Contract (M5 Forward Compatibility)
// =============================================================================
//
// Even though replay is M5, we lock in the schema now. Events and traces
// must store enough metadata to enable future replay functionality.
//
// What breaks if this fails?
// - M5 replay impossible
// - Events lack sequence numbers or timestamps
// - Traces lack parent IDs
// - Replay cannot reconstruct execution order

mod replay_metadata_contract {
    use super::*;

    #[test]
    fn test_eventlog_has_sequence_numbers() {
        // Events have sequence numbers for ordering during replay
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let version = tp
            .event_log
            .append(&run_id, "event", values::null())
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };

        let event = tp.event_log.read(&run_id, seq).unwrap().unwrap();
        assert_eq!(event.value.sequence, seq);
    }

    #[test]
    fn test_eventlog_has_timestamps() {
        // Events have timestamps for temporal ordering
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let version = tp
            .event_log
            .append(&run_id, "event", values::null())
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };

        let event = tp.event_log.read(&run_id, seq).unwrap().unwrap();
        // Timestamp should be non-zero (set at creation time)
        assert!(event.value.timestamp > 0);
    }

    #[test]
    fn test_eventlog_has_event_type() {
        // Events have type for categorization during replay
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let version = tp
            .event_log
            .append(&run_id, "tool_call", values::string("data"))
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };

        let event = tp.event_log.read(&run_id, seq).unwrap().unwrap();
        assert_eq!(event.value.event_type, "tool_call");
    }

    #[test]
    fn test_eventlog_has_prev_hash_for_chain_verification() {
        // Events have prev_hash for chain integrity verification
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.event_log
            .append(&run_id, "first", values::null())
            .unwrap();
        let version = tp
            .event_log
            .append(&run_id, "second", values::null())
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };

        let event = tp.event_log.read(&run_id, seq).unwrap().unwrap();
        // Second event's prev_hash should be non-zero (points to first)
        assert_ne!(event.value.prev_hash, [0u8; 32]);
    }

    #[test]
    fn test_eventlog_has_hash() {
        // Events have their own hash for identity and chain integrity
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let version = tp
            .event_log
            .append(&run_id, "event", values::null())
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };

        let event = tp.event_log.read(&run_id, seq).unwrap().unwrap();
        // Read the event back to get the hash
        assert_ne!(event.value.hash, [0u8; 32]);
    }

    #[test]
    fn test_tracestore_has_trace_type() {
        // Traces have type for categorization
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let trace_id = tp
            .trace_store
            .record(
                &run_id,
                TraceType::Thought {
                    content: "reasoning".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap()
            .value;

        let trace = tp.trace_store.get(&run_id, &trace_id).unwrap().unwrap();
        assert!(matches!(trace.value.trace_type, TraceType::Thought { .. }));
    }

    #[test]
    fn test_tracestore_has_timestamp() {
        // Traces have timestamps
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let trace_id = tp
            .trace_store
            .record(
                &run_id,
                TraceType::Custom {
                    name: "Action".into(),
                    data: values::null(),
                },
                vec![],
                values::null(),
            )
            .unwrap()
            .value;

        let trace = tp.trace_store.get(&run_id, &trace_id).unwrap().unwrap();
        assert!(trace.value.timestamp > 0);
    }

    #[test]
    fn test_tracestore_has_parent_id() {
        // Child traces have parent_id for tree reconstruction
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let parent_id = tp
            .trace_store
            .record(
                &run_id,
                TraceType::Thought {
                    content: "parent".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap()
            .value;
        let child_id = tp
            .trace_store
            .record_child(
                &run_id,
                &parent_id,
                TraceType::Thought {
                    content: "child".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap()
            .value;

        let child = tp.trace_store.get(&run_id, &child_id).unwrap().unwrap();
        assert_eq!(child.value.parent_id, Some(parent_id));
    }

    #[test]
    fn test_tracestore_root_has_no_parent() {
        // Root traces have no parent_id
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let root_id = tp
            .trace_store
            .record(
                &run_id,
                TraceType::Thought {
                    content: "root".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap()
            .value;

        let root = tp.trace_store.get(&run_id, &root_id).unwrap().unwrap();
        assert_eq!(root.value.parent_id, None);
    }

    #[test]
    fn test_run_has_created_at_timestamp() {
        // Runs have creation timestamp
        let tp = TestPrimitives::new();

        let meta = tp.run_index.create_run("test-run").unwrap();
        let run_info = tp.run_index.get_run(&meta.value.name).unwrap().unwrap();

        assert!(run_info.value.created_at > 0);
    }
}

// =============================================================================
// M3.24: No Implicit Coupling Between Primitives
// =============================================================================
//
// Primitives operate independently - no primitive operation implicitly triggers
// another primitive's operation.
//
// What breaks if this fails?
// - Unexpected side effects
// - StateCell update triggers EventLog append
// - Makes reasoning about operations impossible

mod no_implicit_coupling {
    use super::*;

    #[test]
    fn test_kv_put_does_not_create_event() {
        // KV operations don't implicitly create events
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Put several KV values
        tp.kv.put(&run_id, "a", values::int(1)).unwrap();
        tp.kv.put(&run_id, "b", values::int(2)).unwrap();
        tp.kv.put(&run_id, "c", values::int(3)).unwrap();

        // EventLog should be empty
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 0);
    }

    #[test]
    fn test_statecell_update_does_not_create_event() {
        // StateCell operations don't implicitly create events
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();
        tp.state_cell.set(&run_id, "cell", values::int(1)).unwrap();
        tp.state_cell
            .cas(&run_id, "cell", 2, values::int(2))
            .unwrap();

        // EventLog should be empty
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 0);
    }

    #[test]
    fn test_event_append_does_not_create_trace() {
        // EventLog operations don't implicitly create traces
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.event_log
            .append(&run_id, "event_1", values::null())
            .unwrap();
        tp.event_log
            .append(&run_id, "event_2", values::null())
            .unwrap();
        tp.event_log
            .append(&run_id, "event_3", values::null())
            .unwrap();

        // TraceStore should be empty
        assert_eq!(tp.trace_store.count(&run_id).unwrap(), 0);
    }

    #[test]
    fn test_trace_record_does_not_create_event() {
        // TraceStore operations don't implicitly create events
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.trace_store
            .record(
                &run_id,
                TraceType::Thought {
                    content: "t1".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap();
        tp.trace_store
            .record(
                &run_id,
                TraceType::Thought {
                    content: "t2".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap();

        // EventLog should be empty
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 0);
    }

    #[test]
    fn test_primitives_dont_affect_each_other() {
        // Operations on one primitive don't change another
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Record initial counts (KVStore uses list().len() for counting)
        let kv_count_before = tp.kv.list(&run_id, None).unwrap().len();
        let event_count_before = tp.event_log.len(&run_id).unwrap();
        let trace_count_before = tp.trace_store.count(&run_id).unwrap();

        // Do some state cell operations
        tp.state_cell.init(&run_id, "test", values::int(1)).unwrap();
        tp.state_cell.set(&run_id, "test", values::int(2)).unwrap();

        // Other primitive counts unchanged
        assert_eq!(tp.kv.list(&run_id, None).unwrap().len(), kv_count_before);
        assert_eq!(tp.event_log.len(&run_id).unwrap(), event_count_before);
        assert_eq!(tp.trace_store.count(&run_id).unwrap(), trace_count_before);
    }
}
