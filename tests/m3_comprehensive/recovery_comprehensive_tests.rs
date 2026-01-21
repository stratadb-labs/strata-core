//! Recovery Comprehensive Tests (Tier 2)
//!
//! Tests that verify all primitives survive crash+recovery:
//! - Multi-primitive recovery
//! - Sequence continuity after recovery
//! - CAS version continuity after recovery
//! - Index recovery
//! - Multiple recovery cycles

use crate::test_utils::{values, PersistentTestPrimitives};
use strata_core::contract::Version;
use strata_core::value::Value;
use strata_primitives::TraceType;

// =============================================================================
// Multi-Primitive Recovery
// =============================================================================

mod multi_primitive_recovery {
    use super::*;

    #[test]
    fn test_all_primitives_recover() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: Write to all primitives
        {
            let prims = ptp.open();
            prims.kv.put(&run_id, "key1", values::int(100)).unwrap();
            prims
                .kv
                .put(&run_id, "key2", values::string("hello"))
                .unwrap();
            prims
                .event_log
                .append(&run_id, "event1", values::int(1))
                .unwrap();
            prims
                .event_log
                .append(&run_id, "event2", values::int(2))
                .unwrap();
            prims
                .state_cell
                .init(&run_id, "cell", values::int(42))
                .unwrap();
            prims
                .trace_store
                .record(
                    &run_id,
                    TraceType::Thought {
                        content: "thinking".into(),
                        confidence: None,
                    },
                    vec![],
                    values::null(),
                )
                .unwrap();
        }

        // Session 2: Recover and verify
        {
            let prims = ptp.open();

            // KV recovered
            assert_eq!(
                prims.kv.get(&run_id, "key1").unwrap().map(|v| v.value),
                Some(values::int(100))
            );
            assert_eq!(
                prims.kv.get(&run_id, "key2").unwrap().map(|v| v.value),
                Some(values::string("hello"))
            );

            // EventLog recovered
            let events = prims.event_log.read_range(&run_id, 0, 10).unwrap();
            assert_eq!(events.len(), 2);
            assert_eq!(events[0].value.payload, values::int(1));
            assert_eq!(events[1].value.payload, values::int(2));

            // StateCell recovered
            let state = prims.state_cell.read(&run_id, "cell").unwrap().unwrap();
            assert_eq!(state.value.value, values::int(42));

            // TraceStore recovered
            let traces = prims.trace_store.query_by_type(&run_id, "Thought").unwrap();
            assert_eq!(traces.len(), 1);
            assert!(matches!(traces[0].trace_type, TraceType::Thought { .. }));
        }
    }

    #[test]
    fn test_strict_durability_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: Write with strict durability
        {
            let prims = ptp.open_strict();
            prims
                .kv
                .put(&run_id, "strict_key", values::int(999))
                .unwrap();
            prims
                .event_log
                .append(&run_id, "strict_event", values::null())
                .unwrap();
        }

        // Session 2: Recover with strict durability
        {
            let prims = ptp.open_strict();
            assert_eq!(
                prims.kv.get(&run_id, "strict_key").unwrap().map(|v| v.value),
                Some(values::int(999))
            );
            assert_eq!(prims.event_log.len(&run_id).unwrap(), 1);
        }
    }
}

// =============================================================================
// Sequence Continuity After Recovery
// =============================================================================

mod sequence_continuity {
    use super::*;

    #[test]
    fn test_eventlog_sequence_continues_after_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: Append events 0, 1, 2
        {
            let prims = ptp.open();
            let version0 = prims
                .event_log
                .append(&run_id, "event", values::int(0))
                .unwrap();
            let Version::Sequence(seq0) = version0 else { panic!("Expected Sequence version") };
            let version1 = prims
                .event_log
                .append(&run_id, "event", values::int(1))
                .unwrap();
            let Version::Sequence(seq1) = version1 else { panic!("Expected Sequence version") };
            let version2 = prims
                .event_log
                .append(&run_id, "event", values::int(2))
                .unwrap();
            let Version::Sequence(seq2) = version2 else { panic!("Expected Sequence version") };

            assert_eq!(seq0, 0);
            assert_eq!(seq1, 1);
            assert_eq!(seq2, 2);
        }

        // Session 2: Recover and continue appending
        {
            let prims = ptp.open();
            let version3 = prims
                .event_log
                .append(&run_id, "event", values::int(3))
                .unwrap();
            let Version::Sequence(seq3) = version3 else { panic!("Expected Sequence version") };

            // Sequence continues from 3, not 0
            assert_eq!(seq3, 3);

            // All 4 events present
            assert_eq!(prims.event_log.len(&run_id).unwrap(), 4);
        }
    }

    #[test]
    fn test_eventlog_sequences_contiguous_after_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1
        {
            let prims = ptp.open();
            for i in 0..5 {
                prims
                    .event_log
                    .append(&run_id, "event", values::int(i))
                    .unwrap();
            }
        }

        // Session 2
        {
            let prims = ptp.open();
            for i in 5..10 {
                prims
                    .event_log
                    .append(&run_id, "event", values::int(i))
                    .unwrap();
            }
        }

        // Session 3: Verify contiguous
        {
            let prims = ptp.open();
            let events = prims.event_log.read_range(&run_id, 0, 20).unwrap();

            assert_eq!(events.len(), 10);
            for (i, event) in events.iter().enumerate() {
                assert_eq!(
                    event.value.sequence, i as u64,
                    "Gap at index {}: expected {}, got {}",
                    i, i, event.value.sequence
                );
            }
        }
    }
}

// =============================================================================
// CAS Version Continuity After Recovery
// =============================================================================

mod cas_version_continuity {
    use super::*;

    #[test]
    fn test_statecell_version_continues_after_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: Init and CAS multiple times
        let version_after_session_1;
        {
            let prims = ptp.open();
            prims
                .state_cell
                .init(&run_id, "cell", values::int(0))
                .unwrap();

            prims
                .state_cell
                .cas(&run_id, "cell", 1, values::int(1))
                .unwrap();
            prims
                .state_cell
                .cas(&run_id, "cell", 2, values::int(2))
                .unwrap();
            prims
                .state_cell
                .cas(&run_id, "cell", 3, values::int(3))
                .unwrap();

            let state = prims.state_cell.read(&run_id, "cell").unwrap().unwrap();
            version_after_session_1 = state.value.version;
            assert_eq!(state.value.version, 4);
        }

        // Session 2: Recover and verify version
        {
            let prims = ptp.open();
            let state = prims.state_cell.read(&run_id, "cell").unwrap().unwrap();

            assert_eq!(state.value.value, values::int(3));
            assert_eq!(state.value.version, version_after_session_1);

            // CAS with old version fails
            let result = prims.state_cell.cas(&run_id, "cell", 3, values::int(999));
            assert!(result.is_err());

            // CAS with current version succeeds (returns new version)
            let new_version = prims
                .state_cell
                .cas(&run_id, "cell", 4, values::int(4))
                .unwrap()
                .value;
            assert_eq!(new_version, 5);

            let state = prims.state_cell.read(&run_id, "cell").unwrap().unwrap();
            assert_eq!(state.value.version, 5);
        }
    }

    #[test]
    fn test_statecell_transition_after_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: Init and transition
        {
            let prims = ptp.open();
            prims
                .state_cell
                .init(&run_id, "counter", values::int(0))
                .unwrap();
            prims
                .state_cell
                .transition(&run_id, "counter", |state| {
                    if let Value::I64(n) = &state.value {
                        Ok((values::int(n + 10), ()))
                    } else {
                        Ok((values::int(10), ()))
                    }
                })
                .unwrap();
        }

        // Session 2: Continue transitioning
        {
            let prims = ptp.open();

            // Value should be 10
            let state = prims.state_cell.read(&run_id, "counter").unwrap().unwrap();
            assert_eq!(state.value.value, values::int(10));

            // Transition again
            prims
                .state_cell
                .transition(&run_id, "counter", |state| {
                    if let Value::I64(n) = &state.value {
                        Ok((values::int(n + 5), ()))
                    } else {
                        Ok((values::int(5), ()))
                    }
                })
                .unwrap();

            let state = prims.state_cell.read(&run_id, "counter").unwrap().unwrap();
            assert_eq!(state.value.value, values::int(15));
        }
    }
}

// =============================================================================
// Index Recovery
// =============================================================================

mod index_recovery {
    use super::*;

    #[test]
    fn test_tracestore_type_index_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: Record traces of various types
        {
            let prims = ptp.open();
            prims
                .trace_store
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
            prims
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
                .unwrap();
            prims
                .trace_store
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
            prims
                .trace_store
                .record(
                    &run_id,
                    TraceType::Custom {
                        name: "Observation".into(),
                        data: values::null(),
                    },
                    vec![],
                    values::null(),
                )
                .unwrap();
            prims
                .trace_store
                .record(
                    &run_id,
                    TraceType::Thought {
                        content: "t3".into(),
                        confidence: None,
                    },
                    vec![],
                    values::null(),
                )
                .unwrap();
        }

        // Session 2: Query by type
        {
            let prims = ptp.open();
            let thoughts = prims.trace_store.query_by_type(&run_id, "Thought").unwrap();
            let actions = prims.trace_store.query_by_type(&run_id, "Action").unwrap();
            let observations = prims
                .trace_store
                .query_by_type(&run_id, "Observation")
                .unwrap();

            assert_eq!(thoughts.len(), 3);
            assert_eq!(actions.len(), 1);
            assert_eq!(observations.len(), 1);
        }
    }

    #[test]
    fn test_tracestore_parent_child_index_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        let parent_id;
        // Session 1: Create trace tree
        {
            let prims = ptp.open();
            parent_id = prims
                .trace_store
                .record(
                    &run_id,
                    TraceType::Custom {
                        name: "Parent".into(),
                        data: values::null(),
                    },
                    vec![],
                    values::null(),
                )
                .unwrap()
                .value;
            prims
                .trace_store
                .record_child(
                    &run_id,
                    &parent_id,
                    TraceType::Custom {
                        name: "Child1".into(),
                        data: values::null(),
                    },
                    vec![],
                    values::null(),
                )
                .unwrap();
            prims
                .trace_store
                .record_child(
                    &run_id,
                    &parent_id,
                    TraceType::Custom {
                        name: "Child2".into(),
                        data: values::null(),
                    },
                    vec![],
                    values::null(),
                )
                .unwrap();
        }

        // Session 2: Query children
        {
            let prims = ptp.open();
            let children = prims.trace_store.get_children(&run_id, &parent_id).unwrap();
            assert_eq!(children.len(), 2);
        }
    }

    #[test]
    fn test_eventlog_type_index_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: Append events of various types
        {
            let prims = ptp.open();
            prims
                .event_log
                .append(&run_id, "tool_call", values::null())
                .unwrap();
            prims
                .event_log
                .append(&run_id, "response", values::null())
                .unwrap();
            prims
                .event_log
                .append(&run_id, "tool_call", values::null())
                .unwrap();
            prims
                .event_log
                .append(&run_id, "error", values::null())
                .unwrap();
        }

        // Session 2: Query by type
        {
            let prims = ptp.open();
            let tool_calls = prims.event_log.read_by_type(&run_id, "tool_call").unwrap();
            let responses = prims.event_log.read_by_type(&run_id, "response").unwrap();

            assert_eq!(tool_calls.len(), 2);
            assert_eq!(responses.len(), 1);
        }
    }
}

// =============================================================================
// Multiple Recovery Cycles
// =============================================================================

mod multiple_recovery_cycles {
    use super::*;

    #[test]
    fn test_three_recovery_cycles() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Cycle 1: Initial data
        {
            let prims = ptp.open();
            prims.kv.put(&run_id, "cycle", values::int(1)).unwrap();
            prims
                .event_log
                .append(&run_id, "cycle_1", values::null())
                .unwrap();
        }

        // Cycle 2: Recover and add more
        {
            let prims = ptp.open();
            assert_eq!(
                prims.kv.get(&run_id, "cycle").unwrap().map(|v| v.value),
                Some(values::int(1))
            );
            assert_eq!(prims.event_log.len(&run_id).unwrap(), 1);

            prims.kv.put(&run_id, "cycle", values::int(2)).unwrap();
            prims
                .event_log
                .append(&run_id, "cycle_2", values::null())
                .unwrap();
        }

        // Cycle 3: Recover and verify all
        {
            let prims = ptp.open();
            assert_eq!(
                prims.kv.get(&run_id, "cycle").unwrap().map(|v| v.value),
                Some(values::int(2))
            );
            assert_eq!(prims.event_log.len(&run_id).unwrap(), 2);

            prims.kv.put(&run_id, "cycle", values::int(3)).unwrap();
            prims
                .event_log
                .append(&run_id, "cycle_3", values::null())
                .unwrap();
        }

        // Final verification
        {
            let prims = ptp.open();
            assert_eq!(
                prims.kv.get(&run_id, "cycle").unwrap().map(|v| v.value),
                Some(values::int(3))
            );
            let events = prims.event_log.read_range(&run_id, 0, 10).unwrap();
            assert_eq!(events.len(), 3);
            assert_eq!(events[0].value.event_type, "cycle_1");
            assert_eq!(events[1].value.event_type, "cycle_2");
            assert_eq!(events[2].value.event_type, "cycle_3");
        }
    }

    #[test]
    fn test_five_recovery_cycles_all_primitives() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        for cycle in 1..=5i64 {
            {
                let prims = ptp.open();

                // Verify previous cycles
                if cycle > 1 {
                    assert_eq!(
                        prims.kv.get(&run_id, "counter").unwrap().map(|v| v.value),
                        Some(values::int(cycle - 1))
                    );
                    assert_eq!(prims.event_log.len(&run_id).unwrap(), (cycle - 1) as u64);
                }

                // Add data for this cycle
                prims
                    .kv
                    .put(&run_id, "counter", values::int(cycle))
                    .unwrap();
                prims
                    .event_log
                    .append(&run_id, &format!("cycle_{}", cycle), values::null())
                    .unwrap();

                if cycle == 1 {
                    prims
                        .state_cell
                        .init(&run_id, "state", values::int(0))
                        .unwrap();
                }
                prims
                    .state_cell
                    .transition(&run_id, "state", |state| {
                        if let Value::I64(n) = &state.value {
                            Ok((values::int(n + 1), ()))
                        } else {
                            Ok((values::int(1), ()))
                        }
                    })
                    .unwrap();
            }
        }

        // Final verification
        {
            let prims = ptp.open();
            assert_eq!(
                prims.kv.get(&run_id, "counter").unwrap().map(|v| v.value),
                Some(values::int(5))
            );
            assert_eq!(prims.event_log.len(&run_id).unwrap(), 5);
            let state = prims.state_cell.read(&run_id, "state").unwrap().unwrap();
            assert_eq!(state.value.value, values::int(5));
        }
    }
}

// =============================================================================
// Run Index Recovery
// =============================================================================

mod runindex_recovery {
    use super::*;
    use strata_primitives::RunStatus;

    #[test]
    fn test_run_status_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();

        let run_name;
        // Session 1: Create run and update status
        {
            let prims = ptp.open();
            let meta = prims.run_index.create_run("test-run").unwrap();
            run_name = meta.value.name.clone();
            prims
                .run_index
                .update_status(&run_name, RunStatus::Paused)
                .unwrap();
        }

        // Session 2: Verify status
        {
            let prims = ptp.open();
            let run_info = prims.run_index.get_run(&run_name).unwrap().unwrap();
            assert_eq!(run_info.value.status, RunStatus::Paused);
        }
    }

    #[test]
    fn test_run_metadata_survives_recovery() {
        let ptp = PersistentTestPrimitives::new();

        let run_name;
        // Session 1: Create run with metadata (using create_run_with_options)
        {
            let prims = ptp.open();
            let meta = prims
                .run_index
                .create_run_with_options(
                    "tagged-run",
                    None,
                    vec!["tag1".to_string(), "tag2".to_string()],
                    values::null(),
                )
                .unwrap();
            run_name = meta.value.name.clone();
        }

        // Session 2: Verify metadata
        {
            let prims = ptp.open();
            let run_info = prims.run_index.get_run(&run_name).unwrap().unwrap();
            assert!(run_info.value.tags.contains(&"tag1".to_string()));
            assert!(run_info.value.tags.contains(&"tag2".to_string()));
        }
    }

    #[test]
    fn test_multiple_runs_survive_recovery() {
        let ptp = PersistentTestPrimitives::new();

        let run_names: Vec<String>;
        // Session 1: Create multiple runs with different statuses
        {
            let prims = ptp.open();
            let meta0 = prims.run_index.create_run("run-0").unwrap();
            let meta1 = prims.run_index.create_run("run-1").unwrap();
            let meta2 = prims.run_index.create_run("run-2").unwrap();

            run_names = vec![meta0.value.name.clone(), meta1.value.name.clone(), meta2.value.name.clone()];

            prims
                .run_index
                .update_status(&run_names[1], RunStatus::Paused)
                .unwrap();
            prims.run_index.complete_run(&run_names[2]).unwrap();
        }

        // Session 2: Verify all runs and statuses
        {
            let prims = ptp.open();

            let run0 = prims.run_index.get_run(&run_names[0]).unwrap().unwrap();
            assert_eq!(run0.value.status, RunStatus::Active);

            let run1 = prims.run_index.get_run(&run_names[1]).unwrap().unwrap();
            assert_eq!(run1.value.status, RunStatus::Paused);

            let run2 = prims.run_index.get_run(&run_names[2]).unwrap().unwrap();
            assert_eq!(run2.value.status, RunStatus::Completed);
        }
    }
}

// =============================================================================
// Chain Integrity After Recovery
// =============================================================================

mod chain_integrity_after_recovery {
    use super::*;

    #[test]
    fn test_eventlog_chain_valid_after_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: Build chain
        {
            let prims = ptp.open();
            for i in 0..10 {
                prims
                    .event_log
                    .append(&run_id, "event", values::int(i))
                    .unwrap();
            }
        }

        // Session 2: Verify chain integrity
        {
            let prims = ptp.open();
            let result = prims.event_log.verify_chain(&run_id).unwrap();
            assert!(result.is_valid);
        }
    }

    #[test]
    fn test_eventlog_chain_extends_correctly_after_recovery() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Session 1: Start chain
        {
            let prims = ptp.open();
            prims
                .event_log
                .append(&run_id, "first", values::null())
                .unwrap();
        }

        // Session 2: Extend chain
        {
            let prims = ptp.open();
            prims
                .event_log
                .append(&run_id, "second", values::null())
                .unwrap();

            // Chain should still be valid
            let result = prims.event_log.verify_chain(&run_id).unwrap();
            assert!(result.is_valid);

            // Check prev_hash linkage
            let events = prims.event_log.read_range(&run_id, 0, 10).unwrap();
            assert_eq!(events.len(), 2);
            assert_eq!(events[1].value.prev_hash, events[0].value.hash);
        }
    }
}
