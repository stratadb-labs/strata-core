//! Run Isolation Comprehensive Tests (Tier 2)
//!
//! Tests that verify complete isolation between runs:
//! - N-run isolation verification
//! - Concurrent run operations
//! - Run delete isolation
//! - Cross-run data leakage prevention

use crate::test_utils::{concurrent, values, TestPrimitives};
use strata_core::types::RunId;
use strata_primitives::TraceType;
use std::collections::HashSet;

// =============================================================================
// N-Run Isolation
// =============================================================================

mod n_run_isolation {
    use super::*;

    #[test]
    fn test_10_run_isolation() {
        let tp = TestPrimitives::new();
        let runs: Vec<_> = (0..10).map(|_| tp.new_run()).collect();

        // Each run writes to all primitives with unique data
        for (i, run) in runs.iter().enumerate() {
            tp.kv.put(run, "counter", values::int(i as i64)).unwrap();
            tp.kv
                .put(run, "name", values::string(&format!("run_{}", i)))
                .unwrap();
            tp.event_log
                .append(run, &format!("init_{}", i), values::int(i as i64))
                .unwrap();
            tp.state_cell
                .init(run, "state", values::int(i as i64))
                .unwrap();
            tp.trace_store
                .record(
                    run,
                    TraceType::Custom {
                        name: format!("Trace{}", i),
                        data: values::int(i as i64),
                    },
                    vec![],
                    values::null(),
                )
                .unwrap();
        }

        // Verify each run sees only its own data
        for (i, run) in runs.iter().enumerate() {
            // KV isolation
            assert_eq!(
                tp.kv.get(run, "counter").unwrap().map(|v| v.value),
                Some(values::int(i as i64))
            );
            assert_eq!(
                tp.kv.get(run, "name").unwrap().map(|v| v.value),
                Some(values::string(&format!("run_{}", i)))
            );

            // EventLog isolation
            let events = tp.event_log.read_range(run, 0, 100).unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].value.event_type, format!("init_{}", i));

            // StateCell isolation
            let state = tp.state_cell.read(run, "state").unwrap().unwrap();
            assert_eq!(state.value.value, values::int(i as i64));

            // TraceStore isolation
            let traces = tp
                .trace_store
                .query_by_type(run, &format!("Trace{}", i))
                .unwrap();
            assert_eq!(traces.len(), 1);
        }
    }

    #[test]
    fn test_50_run_isolation() {
        let tp = TestPrimitives::new();
        let runs: Vec<_> = (0..50).map(|_| tp.new_run()).collect();

        // Write unique data to each run
        for (i, run) in runs.iter().enumerate() {
            tp.kv.put(run, "id", values::int(i as i64)).unwrap();
        }

        // Verify isolation
        for (i, run) in runs.iter().enumerate() {
            assert_eq!(tp.kv.get(run, "id").unwrap().map(|v| v.value), Some(values::int(i as i64)));
        }
    }

    #[test]
    fn test_shared_key_name_different_runs() {
        // Same key name used across all runs, but isolated
        let tp = TestPrimitives::new();
        let runs: Vec<_> = (0..5).map(|_| tp.new_run()).collect();

        // All runs use same key name
        for (i, run) in runs.iter().enumerate() {
            tp.kv.put(run, "shared_key", values::int(i as i64)).unwrap();
        }

        // Each run has its own value
        for (i, run) in runs.iter().enumerate() {
            assert_eq!(
                tp.kv.get(run, "shared_key").unwrap().map(|v| v.value),
                Some(values::int(i as i64))
            );
        }
    }
}

// =============================================================================
// Concurrent Run Operations
// =============================================================================

mod concurrent_run_operations {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_concurrent_writes_to_different_runs() {
        let tp = Arc::new(TestPrimitives::new());
        let num_threads = 10;

        // Create runs upfront
        let runs: Vec<_> = (0..num_threads).map(|_| tp.new_run()).collect();
        let runs = Arc::new(runs);

        // Each thread writes to its own run
        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), runs.clone()),
            |i, (tp, runs)| {
                let run = &runs[i];
                tp.kv.put(run, "value", values::int(i as i64)).unwrap();
                tp.event_log
                    .append(run, "event", values::int(i as i64))
                    .unwrap();
                i
            },
        );

        // All threads completed
        assert_eq!(results.len(), num_threads);

        // Verify each run has correct data
        for (i, run) in runs.iter().enumerate() {
            assert_eq!(
                tp.kv.get(run, "value").unwrap().map(|v| v.value),
                Some(values::int(i as i64))
            );
            let events = tp.event_log.read_range(run, 0, 10).unwrap();
            assert_eq!(events.len(), 1);
        }
    }

    #[test]
    fn test_concurrent_reads_from_different_runs() {
        let tp = Arc::new(TestPrimitives::new());
        let num_threads = 10;

        // Setup: create runs with data
        let runs: Vec<_> = (0..num_threads)
            .map(|i| {
                let run = tp.new_run();
                tp.kv.put(&run, "value", values::int(i as i64)).unwrap();
                run
            })
            .collect();
        let runs = Arc::new(runs);

        // Concurrent reads
        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), runs.clone()),
            |i, (tp, runs)| {
                let run = &runs[i];
                tp.kv.get(run, "value").unwrap().map(|v| v.value)
            },
        );

        // All reads returned correct values
        for (i, result) in results.iter().enumerate() {
            assert_eq!(*result, Some(values::int(i as i64)));
        }
    }

    #[test]
    fn test_concurrent_create_and_write() {
        let tp = Arc::new(TestPrimitives::new());
        let num_threads = 10;

        // Each thread creates its own run and writes to it
        let results = concurrent::run_with_shared(num_threads, tp.clone(), |i, tp| {
            let run = tp.new_run();
            tp.kv.put(&run, "creator", values::int(i as i64)).unwrap();
            tp.event_log
                .append(&run, "created", values::int(i as i64))
                .unwrap();
            run
        });

        // Verify all runs were created with correct data
        for (i, run) in results.iter().enumerate() {
            assert_eq!(
                tp.kv.get(run, "creator").unwrap().map(|v| v.value),
                Some(values::int(i as i64))
            );
        }
    }
}

// =============================================================================
// Run Delete Isolation
// =============================================================================

mod run_delete_isolation {
    use super::*;

    #[test]
    fn test_delete_run_preserves_other_runs() {
        let tp = TestPrimitives::new();
        // Create runs using new_run() which returns RunId
        let run_a = tp.new_run();
        let run_b = tp.new_run();

        // Also register them in RunIndex for metadata tracking
        let meta_a = tp.run_index.create_run("run-a").unwrap();
        let meta_b = tp.run_index.create_run("run-b").unwrap();

        // Write to both runs
        tp.kv.put(&run_a, "key", values::string("a")).unwrap();
        tp.kv.put(&run_b, "key", values::string("b")).unwrap();
        tp.event_log
            .append(&run_a, "event_a", values::null())
            .unwrap();
        tp.event_log
            .append(&run_b, "event_b", values::null())
            .unwrap();

        // Delete run A from RunIndex
        tp.run_index.delete_run(&meta_a.value.name).unwrap();

        // Run B data untouched
        assert_eq!(tp.kv.get(&run_b, "key").unwrap().map(|v| v.value), Some(values::string("b")));
        let events = tp.event_log.read_range(&run_b, 0, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value.event_type, "event_b");

        // Note: Deleting from RunIndex doesn't delete the actual primitive data
        // That would be a separate cleanup operation
    }

    #[test]
    fn test_multiple_run_metadata_management() {
        let tp = TestPrimitives::new();

        // Create run metadata entries
        let metas: Vec<_> = (0..5)
            .map(|i| tp.run_index.create_run(&format!("run-{}", i)).unwrap())
            .collect();

        // Create actual runs for data storage
        let runs: Vec<_> = (0..5).map(|_| tp.new_run()).collect();

        // Write to all runs
        for (i, run) in runs.iter().enumerate() {
            tp.kv.put(run, "value", values::int(i as i64)).unwrap();
        }

        // Delete some metadata entries (0, 2, 4)
        tp.run_index.delete_run(&metas[0].value.name).unwrap();
        tp.run_index.delete_run(&metas[2].value.name).unwrap();
        tp.run_index.delete_run(&metas[4].value.name).unwrap();

        // Runs 1, 3 still exist in index
        assert!(tp.run_index.exists(&metas[1].value.name).unwrap());
        assert!(tp.run_index.exists(&metas[3].value.name).unwrap());

        // Deleted runs no longer in index
        assert!(!tp.run_index.exists(&metas[0].value.name).unwrap());
        assert!(!tp.run_index.exists(&metas[2].value.name).unwrap());
        assert!(!tp.run_index.exists(&metas[4].value.name).unwrap());
    }

    #[test]
    fn test_all_primitives_isolation_across_runs() {
        let tp = TestPrimitives::new();
        let run_a = tp.new_run();
        let run_b = tp.new_run();

        // Write to all primitives for both runs
        for run in [&run_a, &run_b] {
            tp.kv.put(run, "kv_key", values::int(1)).unwrap();
            tp.event_log.append(run, "event", values::null()).unwrap();
            tp.state_cell.init(run, "cell", values::int(0)).unwrap();
            tp.trace_store
                .record(
                    run,
                    TraceType::Thought {
                        content: "thinking".into(),
                        confidence: None,
                    },
                    vec![],
                    values::null(),
                )
                .unwrap();
        }

        // Both runs have data
        assert!(tp.kv.get(&run_a, "kv_key").unwrap().is_some());
        assert!(tp.kv.get(&run_b, "kv_key").unwrap().is_some());
        assert_eq!(tp.event_log.len(&run_a).unwrap(), 1);
        assert_eq!(tp.event_log.len(&run_b).unwrap(), 1);
        assert!(tp.state_cell.read(&run_a, "cell").unwrap().is_some());
        assert!(tp.state_cell.read(&run_b, "cell").unwrap().is_some());
        assert_eq!(tp.trace_store.count(&run_a).unwrap(), 1);
        assert_eq!(tp.trace_store.count(&run_b).unwrap(), 1);
    }
}

// =============================================================================
// Cross-Run Data Leakage Prevention
// =============================================================================

mod cross_run_leakage_prevention {
    use super::*;

    #[test]
    fn test_kv_list_returns_only_run_keys() {
        let tp = TestPrimitives::new();
        let run1 = tp.new_run();
        let run2 = tp.new_run();

        // Write different keys to each run
        tp.kv.put(&run1, "run1_key1", values::int(1)).unwrap();
        tp.kv.put(&run1, "run1_key2", values::int(2)).unwrap();
        tp.kv.put(&run2, "run2_key1", values::int(3)).unwrap();
        tp.kv.put(&run2, "run2_key2", values::int(4)).unwrap();
        tp.kv.put(&run2, "run2_key3", values::int(5)).unwrap();

        // list() returns only keys for that run
        let run1_keys = tp.kv.list(&run1, None).unwrap();
        let run2_keys = tp.kv.list(&run2, None).unwrap();

        assert_eq!(run1_keys.len(), 2);
        assert!(run1_keys.contains(&"run1_key1".to_string()));
        assert!(run1_keys.contains(&"run1_key2".to_string()));

        assert_eq!(run2_keys.len(), 3);
        assert!(run2_keys.contains(&"run2_key1".to_string()));
        assert!(run2_keys.contains(&"run2_key2".to_string()));
        assert!(run2_keys.contains(&"run2_key3".to_string()));
    }

    #[test]
    fn test_eventlog_read_range_returns_only_run_events() {
        let tp = TestPrimitives::new();
        let run1 = tp.new_run();
        let run2 = tp.new_run();

        // Append events to each run
        tp.event_log
            .append(&run1, "run1_event", values::null())
            .unwrap();
        tp.event_log
            .append(&run2, "run2_event_1", values::null())
            .unwrap();
        tp.event_log
            .append(&run2, "run2_event_2", values::null())
            .unwrap();

        // read_range() returns only events for that run
        let run1_events = tp.event_log.read_range(&run1, 0, 100).unwrap();
        let run2_events = tp.event_log.read_range(&run2, 0, 100).unwrap();

        assert_eq!(run1_events.len(), 1);
        assert_eq!(run1_events[0].value.event_type, "run1_event");

        assert_eq!(run2_events.len(), 2);
    }

    #[test]
    fn test_statecell_list_returns_only_run_cells() {
        let tp = TestPrimitives::new();
        let run1 = tp.new_run();
        let run2 = tp.new_run();

        // Init cells in each run
        tp.state_cell.init(&run1, "cell_a", values::int(1)).unwrap();
        tp.state_cell.init(&run2, "cell_b", values::int(2)).unwrap();
        tp.state_cell.init(&run2, "cell_c", values::int(3)).unwrap();

        // list() returns only cells for that run
        let run1_cells = tp.state_cell.list(&run1).unwrap();
        let run2_cells = tp.state_cell.list(&run2).unwrap();

        assert_eq!(run1_cells.len(), 1);
        assert!(run1_cells.contains(&"cell_a".to_string()));

        assert_eq!(run2_cells.len(), 2);
        assert!(run2_cells.contains(&"cell_b".to_string()));
        assert!(run2_cells.contains(&"cell_c".to_string()));
    }

    #[test]
    fn test_tracestore_list_returns_only_run_traces() {
        let tp = TestPrimitives::new();
        let run1 = tp.new_run();
        let run2 = tp.new_run();

        // Record traces in each run
        let id1 = tp
            .trace_store
            .record(
                &run1,
                TraceType::Thought {
                    content: "T1".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap()
            .value;
        let id2 = tp
            .trace_store
            .record(
                &run2,
                TraceType::Thought {
                    content: "T2".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap()
            .value;
        let id3 = tp
            .trace_store
            .record(
                &run2,
                TraceType::Thought {
                    content: "T3".into(),
                    confidence: None,
                },
                vec![],
                values::null(),
            )
            .unwrap()
            .value;

        // list() returns only traces for that run
        let run1_traces = tp.trace_store.list(&run1).unwrap();
        let run2_traces = tp.trace_store.list(&run2).unwrap();

        assert_eq!(run1_traces.len(), 1);
        assert!(run1_traces.iter().any(|t| t.id == id1));

        assert_eq!(run2_traces.len(), 2);
        assert!(run2_traces.iter().any(|t| t.id == id2));
        assert!(run2_traces.iter().any(|t| t.id == id3));
    }

    #[test]
    fn test_run_operations_cannot_access_other_run_data() {
        let tp = TestPrimitives::new();
        let run1 = tp.new_run();
        let run2 = tp.new_run();

        // Write to run1
        tp.kv
            .put(&run1, "secret", values::string("run1_secret"))
            .unwrap();

        // run2 cannot see run1's data
        assert!(tp.kv.get(&run2, "secret").unwrap().is_none());

        // Writing to run2 with same key doesn't affect run1
        tp.kv
            .put(&run2, "secret", values::string("run2_secret"))
            .unwrap();
        assert_eq!(
            tp.kv.get(&run1, "secret").unwrap().map(|v| v.value),
            Some(values::string("run1_secret"))
        );
        assert_eq!(
            tp.kv.get(&run2, "secret").unwrap().map(|v| v.value),
            Some(values::string("run2_secret"))
        );
    }
}

// =============================================================================
// Run ID Uniqueness
// =============================================================================

mod run_id_uniqueness {
    use super::*;

    #[test]
    fn test_run_ids_are_unique() {
        let tp = TestPrimitives::new();
        let mut ids = HashSet::new();

        // Create many runs
        for _ in 0..100 {
            let run = tp.new_run();
            assert!(ids.insert(run), "Duplicate run ID generated");
        }
    }

    #[test]
    fn test_concurrent_run_creation_unique_ids() {
        use std::sync::Arc;

        let tp = Arc::new(TestPrimitives::new());
        let num_threads = 10;
        let runs_per_thread = 10;

        let all_ids = concurrent::run_with_shared(num_threads, tp.clone(), move |_, tp| {
            let mut ids = Vec::new();
            for _ in 0..runs_per_thread {
                ids.push(tp.new_run());
            }
            ids
        });

        // Flatten and check uniqueness
        let mut all: HashSet<RunId> = HashSet::new();
        for thread_ids in all_ids {
            for id in thread_ids {
                assert!(all.insert(id), "Duplicate run ID from concurrent creation");
            }
        }

        assert_eq!(all.len(), num_threads * runs_per_thread);
    }
}
