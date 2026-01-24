//! Primitive API Tests (Tier 2)
//!
//! Comprehensive API coverage for each primitive:
//! - KVStore: get/put/delete/list/list_with_values
//! - EventLog: append/read/read_range/head/len/is_empty/verify_chain/read_by_type
//! - StateCell: init/read/cas/set/delete/exists/transition/transition_or_init
//! - RunIndex: create_run/get_run/update_status/fail_run/complete_run/delete_run

use crate::test_utils::{values, TestPrimitives};
use strata_core::contract::Version;
use strata_core::value::Value;
use strata_primitives::{RunStatus};

// =============================================================================
// KVStore API Tests
// =============================================================================

mod kvstore_api {
    use super::*;

    #[test]
    fn test_get_nonexistent_key_returns_none() {
        let tp = TestPrimitives::new();
        let result = tp.kv.get(&tp.run_id, "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_put_and_get() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "key", values::int(42)).unwrap();
        let result = tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value);
        assert_eq!(result, Some(values::int(42)));
    }

    #[test]
    fn test_put_overwrites() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "key", values::int(1)).unwrap();
        tp.kv.put(&tp.run_id, "key", values::int(2)).unwrap();
        let result = tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value);
        assert_eq!(result, Some(values::int(2)));
    }

    #[test]
    fn test_delete() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "key", values::int(42)).unwrap();
        let deleted = tp.kv.delete(&tp.run_id, "key").unwrap();
        assert!(deleted);
        let result = tp.kv.get(&tp.run_id, "key").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete_nonexistent_returns_false() {
        let tp = TestPrimitives::new();
        // Deleting a nonexistent key returns false
        let deleted = tp.kv.delete(&tp.run_id, "nonexistent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_list_empty() {
        let tp = TestPrimitives::new();
        let keys = tp.kv.list(&tp.run_id, None).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn test_list_all_keys() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "a", values::int(1)).unwrap();
        tp.kv.put(&tp.run_id, "b", values::int(2)).unwrap();
        tp.kv.put(&tp.run_id, "c", values::int(3)).unwrap();

        let keys = tp.kv.list(&tp.run_id, None).unwrap();
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn test_list_with_prefix() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "user:1", values::int(1)).unwrap();
        tp.kv.put(&tp.run_id, "user:2", values::int(2)).unwrap();
        tp.kv.put(&tp.run_id, "item:1", values::int(3)).unwrap();

        let user_keys = tp.kv.list(&tp.run_id, Some("user:")).unwrap();
        assert_eq!(user_keys.len(), 2);

        let item_keys = tp.kv.list(&tp.run_id, Some("item:")).unwrap();
        assert_eq!(item_keys.len(), 1);
    }

    #[test]
    fn test_list_with_values() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "x", values::int(10)).unwrap();
        tp.kv.put(&tp.run_id, "y", values::int(20)).unwrap();

        let pairs = tp.kv.list_with_values(&tp.run_id, None).unwrap();
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn test_various_value_types() {
        let tp = TestPrimitives::new();

        // I64
        tp.kv.put(&tp.run_id, "int", values::int(42)).unwrap();
        assert_eq!(tp.kv.get(&tp.run_id, "int").unwrap().map(|v| v.value), Some(values::int(42)));

        // F64
        tp.kv.put(&tp.run_id, "float", values::float(3.14)).unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "float").unwrap().map(|v| v.value),
            Some(values::float(3.14))
        );

        // String
        tp.kv
            .put(&tp.run_id, "string", values::string("hello"))
            .unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "string").unwrap().map(|v| v.value),
            Some(values::string("hello"))
        );

        // Bool
        tp.kv
            .put(&tp.run_id, "bool", values::bool_val(true))
            .unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "bool").unwrap().map(|v| v.value),
            Some(values::bool_val(true))
        );

        // Null
        tp.kv.put(&tp.run_id, "null", values::null()).unwrap();
        assert_eq!(tp.kv.get(&tp.run_id, "null").unwrap().map(|v| v.value), Some(values::null()));

        // Bytes
        tp.kv
            .put(&tp.run_id, "bytes", values::bytes(&[1, 2, 3]))
            .unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "bytes").unwrap().map(|v| v.value),
            Some(values::bytes(&[1, 2, 3]))
        );

        // Array
        tp.kv
            .put(
                &tp.run_id,
                "array",
                values::array(vec![values::int(1), values::int(2)]),
            )
            .unwrap();

        // Map
        tp.kv
            .put(
                &tp.run_id,
                "map",
                values::map(vec![("a", values::int(1)), ("b", values::int(2))]),
            )
            .unwrap();
    }

    #[test]
    fn test_exists() {
        let tp = TestPrimitives::new();

        assert!(!tp.kv.exists(&tp.run_id, "key").unwrap());

        tp.kv.put(&tp.run_id, "key", values::int(1)).unwrap();
        assert!(tp.kv.exists(&tp.run_id, "key").unwrap());

        tp.kv.delete(&tp.run_id, "key").unwrap();
        assert!(!tp.kv.exists(&tp.run_id, "key").unwrap());
    }

    #[test]
    fn test_count() {
        let tp = TestPrimitives::new();

        // KVStore doesn't have count() - use list().len() instead
        assert_eq!(tp.kv.list(&tp.run_id, None).unwrap().len(), 0);

        tp.kv.put(&tp.run_id, "a", values::int(1)).unwrap();
        tp.kv.put(&tp.run_id, "b", values::int(2)).unwrap();
        assert_eq!(tp.kv.list(&tp.run_id, None).unwrap().len(), 2);
    }
}

// =============================================================================
// EventLog API Tests
// =============================================================================

mod eventlog_api {
    use super::*;

    #[test]
    fn test_append_returns_sequence_version() {
        let tp = TestPrimitives::new();
        let version = tp
            .event_log
            .append(&tp.run_id, "event", values::null())
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };
        assert_eq!(seq, 0);
        // Read back to verify hash is non-zero
        let event = tp.event_log.read(&tp.run_id, seq).unwrap().unwrap();
        assert_ne!(event.value.hash, [0u8; 32]);
    }

    #[test]
    fn test_read_single_event() {
        let tp = TestPrimitives::new();
        let version = tp
            .event_log
            .append(&tp.run_id, "test_event", values::int(42))
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };

        let event = tp.event_log.read(&tp.run_id, seq).unwrap().unwrap();
        assert_eq!(event.value.sequence, seq);
        assert_eq!(event.value.event_type, "test_event");
        assert_eq!(event.value.payload, values::int(42));
    }

    #[test]
    fn test_read_nonexistent_returns_none() {
        let tp = TestPrimitives::new();
        let result = tp.event_log.read(&tp.run_id, 999).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_range() {
        let tp = TestPrimitives::new();
        for i in 0..5 {
            tp.event_log
                .append(&tp.run_id, &format!("event_{}", i), values::int(i))
                .unwrap();
        }

        // read_range(start, end) reads from start to end (exclusive)
        let events = tp.event_log.read_range(&tp.run_id, 1, 4).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].value.sequence, 1);
        assert_eq!(events[1].value.sequence, 2);
        assert_eq!(events[2].value.sequence, 3);
    }

    #[test]
    fn test_read_range_empty() {
        let tp = TestPrimitives::new();
        let events = tp.event_log.read_range(&tp.run_id, 0, 10).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_head_returns_most_recent() {
        let tp = TestPrimitives::new();
        tp.event_log
            .append(&tp.run_id, "first", values::null())
            .unwrap();
        tp.event_log
            .append(&tp.run_id, "second", values::null())
            .unwrap();
        tp.event_log
            .append(&tp.run_id, "third", values::null())
            .unwrap();

        let head = tp.event_log.head(&tp.run_id).unwrap().unwrap();
        assert_eq!(head.value.event_type, "third");
        assert_eq!(head.value.sequence, 2);
    }

    #[test]
    fn test_head_empty_log_returns_none() {
        let tp = TestPrimitives::new();
        let head = tp.event_log.head(&tp.run_id).unwrap();
        assert_eq!(head, None);
    }

    #[test]
    fn test_len() {
        let tp = TestPrimitives::new();
        assert_eq!(tp.event_log.len(&tp.run_id).unwrap(), 0);

        tp.event_log
            .append(&tp.run_id, "e1", values::null())
            .unwrap();
        assert_eq!(tp.event_log.len(&tp.run_id).unwrap(), 1);

        tp.event_log
            .append(&tp.run_id, "e2", values::null())
            .unwrap();
        assert_eq!(tp.event_log.len(&tp.run_id).unwrap(), 2);
    }

    #[test]
    fn test_is_empty() {
        let tp = TestPrimitives::new();
        assert!(tp.event_log.is_empty(&tp.run_id).unwrap());

        tp.event_log
            .append(&tp.run_id, "e", values::null())
            .unwrap();
        assert!(!tp.event_log.is_empty(&tp.run_id).unwrap());
    }

    #[test]
    fn test_verify_chain() {
        let tp = TestPrimitives::new();
        for _ in 0..5 {
            tp.event_log
                .append(&tp.run_id, "event", values::null())
                .unwrap();
        }

        let result = tp.event_log.verify_chain(&tp.run_id).unwrap();
        assert!(result.is_valid);
        assert_eq!(result.length, 5);
    }

    #[test]
    fn test_read_by_type() {
        let tp = TestPrimitives::new();
        tp.event_log
            .append(&tp.run_id, "type_a", values::null())
            .unwrap();
        tp.event_log
            .append(&tp.run_id, "type_b", values::null())
            .unwrap();
        tp.event_log
            .append(&tp.run_id, "type_a", values::null())
            .unwrap();
        tp.event_log
            .append(&tp.run_id, "type_c", values::null())
            .unwrap();
        tp.event_log
            .append(&tp.run_id, "type_a", values::null())
            .unwrap();

        let type_a_events = tp.event_log.read_by_type(&tp.run_id, "type_a").unwrap();
        assert_eq!(type_a_events.len(), 3);
        for event in &type_a_events {
            assert_eq!(event.value.event_type, "type_a");
        }
    }

    #[test]
    fn test_event_types() {
        let tp = TestPrimitives::new();
        tp.event_log
            .append(&tp.run_id, "alpha", values::null())
            .unwrap();
        tp.event_log
            .append(&tp.run_id, "beta", values::null())
            .unwrap();
        tp.event_log
            .append(&tp.run_id, "alpha", values::null())
            .unwrap();

        let types = tp.event_log.event_types(&tp.run_id).unwrap();
        assert_eq!(types.len(), 2);
        assert!(types.contains(&"alpha".to_string()));
        assert!(types.contains(&"beta".to_string()));
    }
}

// =============================================================================
// StateCell API Tests
// =============================================================================

mod statecell_api {
    use super::*;

    #[test]
    fn test_init_creates_cell() {
        let tp = TestPrimitives::new();
        let version = tp
            .state_cell
            .init(&tp.run_id, "cell", values::int(0))
            .unwrap()
            .value;
        assert_eq!(version, 1);

        let state = tp.state_cell.read(&tp.run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(0));
        assert_eq!(state.value.version, 1);
    }

    #[test]
    fn test_init_fails_if_exists() {
        let tp = TestPrimitives::new();
        tp.state_cell
            .init(&tp.run_id, "cell", values::int(0))
            .unwrap();

        let result = tp.state_cell.init(&tp.run_id, "cell", values::int(1));
        assert!(result.is_err());
    }

    #[test]
    fn test_read_nonexistent_returns_none() {
        let tp = TestPrimitives::new();
        let result = tp.state_cell.read(&tp.run_id, "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_cas_success() {
        let tp = TestPrimitives::new();
        tp.state_cell
            .init(&tp.run_id, "cell", values::int(10))
            .unwrap();

        let new_version = tp
            .state_cell
            .cas(&tp.run_id, "cell", 1, values::int(20))
            .unwrap()
            .value;
        assert_eq!(new_version, 2);

        let state = tp.state_cell.read(&tp.run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(20));
        assert_eq!(state.value.version, 2);
    }

    #[test]
    fn test_cas_fails_on_wrong_version() {
        let tp = TestPrimitives::new();
        tp.state_cell
            .init(&tp.run_id, "cell", values::int(10))
            .unwrap();

        let result = tp.state_cell.cas(&tp.run_id, "cell", 999, values::int(20));
        assert!(result.is_err());

        // Value unchanged
        let state = tp.state_cell.read(&tp.run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(10));
    }

    #[test]
    fn test_set_unconditional() {
        let tp = TestPrimitives::new();
        tp.state_cell
            .init(&tp.run_id, "cell", values::int(10))
            .unwrap();

        let new_version = tp
            .state_cell
            .set(&tp.run_id, "cell", values::int(100))
            .unwrap()
            .value;
        assert!(new_version > 1);

        let state = tp.state_cell.read(&tp.run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(100));
    }

    #[test]
    fn test_set_creates_if_not_exists() {
        let tp = TestPrimitives::new();

        // set() on non-existent cell should create it
        let version = tp
            .state_cell
            .set(&tp.run_id, "new_cell", values::int(42))
            .unwrap()
            .value;
        assert!(version >= 1);

        let state = tp.state_cell.read(&tp.run_id, "new_cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(42));
    }

    #[test]
    fn test_delete_cell() {
        let tp = TestPrimitives::new();
        tp.state_cell
            .init(&tp.run_id, "cell", values::int(0))
            .unwrap();

        assert!(tp.state_cell.delete(&tp.run_id, "cell").unwrap());

        let result = tp.state_cell.read(&tp.run_id, "cell").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_delete_nonexistent() {
        let tp = TestPrimitives::new();
        assert!(!tp.state_cell.delete(&tp.run_id, "nonexistent").unwrap());
    }

    #[test]
    fn test_exists() {
        let tp = TestPrimitives::new();
        assert!(!tp.state_cell.exists(&tp.run_id, "cell").unwrap());

        tp.state_cell
            .init(&tp.run_id, "cell", values::int(0))
            .unwrap();
        assert!(tp.state_cell.exists(&tp.run_id, "cell").unwrap());

        tp.state_cell.delete(&tp.run_id, "cell").unwrap();
        assert!(!tp.state_cell.exists(&tp.run_id, "cell").unwrap());
    }

    #[test]
    fn test_transition() {
        let tp = TestPrimitives::new();
        tp.state_cell
            .init(&tp.run_id, "counter", values::int(0))
            .unwrap();

        let result = tp
            .state_cell
            .transition(&tp.run_id, "counter", |state| {
                let current = if let Value::Int(n) = &state.value {
                    *n
                } else {
                    0
                };
                Ok((values::int(current + 1), current + 1))
            })
            .unwrap();

        assert_eq!(result.0, 1);

        let state = tp.state_cell.read(&tp.run_id, "counter").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(1));
    }

    #[test]
    fn test_transition_or_init() {
        let tp = TestPrimitives::new();

        // On non-existent cell, should init then transition
        let (result, _version) = tp
            .state_cell
            .transition_or_init(&tp.run_id, "counter", values::int(0), |state| {
                let current = if let Value::Int(n) = &state.value {
                    *n
                } else {
                    0
                };
                Ok((values::int(current + 10), current + 10))
            })
            .unwrap();

        assert_eq!(result, 10);

        let state = tp.state_cell.read(&tp.run_id, "counter").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(10));
    }
}

// =============================================================================
// RunIndex API Tests
// =============================================================================

mod runindex_api {
    use super::*;

    #[test]
    fn test_create_run() {
        let tp = TestPrimitives::new();
        let meta = tp.run_index.create_run("my-run").unwrap();

        assert_eq!(meta.value.name, "my-run");
        assert_eq!(meta.value.status, RunStatus::Active);
        assert!(meta.value.parent_run.is_none());
    }

    #[test]
    fn test_create_run_with_options() {
        let tp = TestPrimitives::new();

        // Create parent first
        tp.run_index.create_run("parent").unwrap();

        // Create child with options
        let meta = tp
            .run_index
            .create_run_with_options(
                "child",
                Some("parent".to_string()),
                vec!["tag1".to_string(), "tag2".to_string()],
                values::map(vec![("key", values::int(42))]),
            )
            .unwrap();

        assert_eq!(meta.value.name, "child");
        assert_eq!(meta.value.parent_run, Some("parent".to_string()));
        assert_eq!(meta.value.tags, vec!["tag1".to_string(), "tag2".to_string()]);
    }

    #[test]
    fn test_create_duplicate_run_fails() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("my-run").unwrap();

        let result = tp.run_index.create_run("my-run");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_run() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("my-run").unwrap();

        let meta = tp.run_index.get_run("my-run").unwrap();
        assert!(meta.is_some());
        assert_eq!(meta.unwrap().value.name, "my-run");
    }

    #[test]
    fn test_get_nonexistent_run() {
        let tp = TestPrimitives::new();
        let meta = tp.run_index.get_run("nonexistent").unwrap();
        assert!(meta.is_none());
    }

    #[test]
    fn test_exists() {
        let tp = TestPrimitives::new();
        assert!(!tp.run_index.exists("my-run").unwrap());

        tp.run_index.create_run("my-run").unwrap();
        assert!(tp.run_index.exists("my-run").unwrap());
    }

    #[test]
    fn test_list_runs() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("run-1").unwrap();
        tp.run_index.create_run("run-2").unwrap();
        tp.run_index.create_run("run-3").unwrap();

        let runs = tp.run_index.list_runs().unwrap();
        assert_eq!(runs.len(), 3);
    }

    #[test]
    fn test_count() {
        let tp = TestPrimitives::new();
        assert_eq!(tp.run_index.count().unwrap(), 0);

        tp.run_index.create_run("run-1").unwrap();
        tp.run_index.create_run("run-2").unwrap();
        assert_eq!(tp.run_index.count().unwrap(), 2);
    }

    #[test]
    fn test_update_status() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("my-run").unwrap();

        let meta = tp
            .run_index
            .update_status("my-run", RunStatus::Paused)
            .unwrap();
        assert_eq!(meta.value.status, RunStatus::Paused);
    }

    #[test]
    fn test_complete_run() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("my-run").unwrap();

        let meta = tp.run_index.complete_run("my-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Completed);
        assert!(meta.value.completed_at.is_some());
    }

    #[test]
    fn test_fail_run() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("my-run").unwrap();

        let meta = tp.run_index.fail_run("my-run", "error message").unwrap();
        assert_eq!(meta.value.status, RunStatus::Failed);
        assert_eq!(meta.value.error, Some("error message".to_string()));
    }

    #[test]
    fn test_cancel_run() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("my-run").unwrap();

        let meta = tp.run_index.cancel_run("my-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Cancelled);
    }

    #[test]
    fn test_pause_and_resume() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("my-run").unwrap();

        let meta = tp.run_index.pause_run("my-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Paused);

        let meta = tp.run_index.resume_run("my-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Active);
    }

    #[test]
    fn test_archive_run() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("my-run").unwrap();

        let meta = tp.run_index.archive_run("my-run").unwrap();
        assert_eq!(meta.value.status, RunStatus::Archived);
    }

    #[test]
    fn test_delete_run() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("my-run").unwrap();
        assert!(tp.run_index.exists("my-run").unwrap());

        tp.run_index.delete_run("my-run").unwrap();
        assert!(!tp.run_index.exists("my-run").unwrap());
    }

    #[test]
    fn test_query_by_status() {
        let tp = TestPrimitives::new();
        tp.run_index.create_run("active-1").unwrap();
        tp.run_index.create_run("active-2").unwrap();
        tp.run_index.create_run("completed-1").unwrap();
        tp.run_index.complete_run("completed-1").unwrap();

        let active = tp.run_index.query_by_status(RunStatus::Active).unwrap();
        assert_eq!(active.len(), 2);

        let completed = tp.run_index.query_by_status(RunStatus::Completed).unwrap();
        assert_eq!(completed.len(), 1);
    }

    #[test]
    fn test_query_by_tag() {
        let tp = TestPrimitives::new();
        tp.run_index
            .create_run_with_options("run-1", None, vec!["experiment".to_string()], Value::Null)
            .unwrap();
        tp.run_index
            .create_run_with_options(
                "run-2",
                None,
                vec!["experiment".to_string(), "v2".to_string()],
                Value::Null,
            )
            .unwrap();
        tp.run_index.create_run("run-3").unwrap();

        let experiment_runs = tp.run_index.query_by_tag("experiment").unwrap();
        assert_eq!(experiment_runs.len(), 2);

        let v2_runs = tp.run_index.query_by_tag("v2").unwrap();
        assert_eq!(v2_runs.len(), 1);
    }
}
