//! Edge Case Tests (Tier 2)
//!
//! Tests boundary conditions for all primitives:
//! - Empty state tests
//! - Boundary values
//! - Unicode and special characters
//! - Concurrent edge cases

use crate::test_utils::{concurrent, values, TestPrimitives};
use strata_core::contract::Version;
use strata_core::value::Value;
use std::sync::Arc;

// =============================================================================
// Empty State Tests
// =============================================================================

mod empty_state {
    use super::*;

    #[test]
    fn test_kv_get_nonexistent_key() {
        let tp = TestPrimitives::new();
        let result = tp.kv.get(&tp.run_id, "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_kv_list_empty() {
        let tp = TestPrimitives::new();
        let keys = tp.kv.list(&tp.run_id, None).unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn test_eventlog_read_nonexistent_sequence() {
        let tp = TestPrimitives::new();
        let result = tp.event_log.read(&tp.run_id, 0).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_eventlog_head_empty() {
        let tp = TestPrimitives::new();
        let result = tp.event_log.head(&tp.run_id).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_eventlog_len_empty() {
        let tp = TestPrimitives::new();
        assert_eq!(tp.event_log.len(&tp.run_id).unwrap(), 0);
    }

    #[test]
    fn test_eventlog_read_range_empty() {
        let tp = TestPrimitives::new();
        let events = tp.event_log.read_range(&tp.run_id, 0, 100).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_statecell_read_nonexistent() {
        let tp = TestPrimitives::new();
        let result = tp.state_cell.read(&tp.run_id, "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_statecell_exists_nonexistent() {
        let tp = TestPrimitives::new();
        assert!(!tp.state_cell.exists(&tp.run_id, "nonexistent").unwrap());
    }

    #[test]
    fn test_statecell_list_empty() {
        let tp = TestPrimitives::new();
        let cells = tp.state_cell.list(&tp.run_id).unwrap();
        assert!(cells.is_empty());
    }

    #[test]
    fn test_runindex_get_nonexistent_run() {
        let tp = TestPrimitives::new();
        // RunIndex uses string names for runs
        let result = tp.run_index.get_run("nonexistent-run").unwrap();
        assert_eq!(result, None);
    }
}

// =============================================================================
// Boundary Values
// =============================================================================

mod boundary_values {
    use super::*;

    #[test]
    fn test_kv_empty_key() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "", values::int(42)).unwrap();
        assert_eq!(tp.kv.get(&tp.run_id, "").unwrap().map(|v| v.value), Some(values::int(42)));
    }

    #[test]
    fn test_kv_long_key() {
        let tp = TestPrimitives::new();
        let long_key = "a".repeat(1000);
        tp.kv.put(&tp.run_id, &long_key, values::int(1)).unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, &long_key).unwrap().map(|v| v.value),
            Some(values::int(1))
        );
    }

    #[test]
    fn test_kv_empty_string_value() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "key", values::string("")).unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value),
            Some(values::string(""))
        );
    }

    #[test]
    fn test_kv_empty_bytes_value() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "key", values::bytes(&[])).unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value),
            Some(values::bytes(&[]))
        );
    }

    #[test]
    fn test_kv_large_bytes_value() {
        let tp = TestPrimitives::new();
        let large_bytes = vec![0u8; 100_000];
        tp.kv
            .put(&tp.run_id, "key", values::bytes(&large_bytes))
            .unwrap();
        let result = tp.kv.get(&tp.run_id, "key").unwrap().unwrap().value;
        if let Value::Bytes(bytes) = result {
            assert_eq!(bytes.len(), 100_000);
        } else {
            panic!("Expected bytes");
        }
    }

    #[test]
    fn test_kv_i64_min_max() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "min", values::int(i64::MIN)).unwrap();
        tp.kv.put(&tp.run_id, "max", values::int(i64::MAX)).unwrap();

        assert_eq!(
            tp.kv.get(&tp.run_id, "min").unwrap().map(|v| v.value),
            Some(values::int(i64::MIN))
        );
        assert_eq!(
            tp.kv.get(&tp.run_id, "max").unwrap().map(|v| v.value),
            Some(values::int(i64::MAX))
        );
    }

    #[test]
    fn test_kv_f64_special_values() {
        let tp = TestPrimitives::new();

        // Note: NaN != NaN, so we test differently
        tp.kv
            .put(&tp.run_id, "inf", values::float(f64::INFINITY))
            .unwrap();
        tp.kv
            .put(&tp.run_id, "neg_inf", values::float(f64::NEG_INFINITY))
            .unwrap();
        tp.kv.put(&tp.run_id, "zero", values::float(0.0)).unwrap();

        assert_eq!(
            tp.kv.get(&tp.run_id, "inf").unwrap().map(|v| v.value),
            Some(values::float(f64::INFINITY))
        );
        assert_eq!(
            tp.kv.get(&tp.run_id, "neg_inf").unwrap().map(|v| v.value),
            Some(values::float(f64::NEG_INFINITY))
        );
    }

    #[test]
    fn test_kv_empty_array() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "key", values::array(vec![])).unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value),
            Some(values::array(vec![]))
        );
    }

    #[test]
    fn test_kv_nested_array() {
        let tp = TestPrimitives::new();
        let nested = values::array(vec![
            values::array(vec![values::int(1), values::int(2)]),
            values::array(vec![values::int(3), values::int(4)]),
        ]);
        tp.kv.put(&tp.run_id, "key", nested.clone()).unwrap();
        assert_eq!(tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value), Some(nested));
    }

    #[test]
    fn test_kv_empty_map() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "key", values::map(vec![])).unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value),
            Some(values::map(vec![]))
        );
    }

    #[test]
    fn test_eventlog_empty_event_type() {
        let tp = TestPrimitives::new();
        let version = tp.event_log.append(&tp.run_id, "", values::null()).unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };
        let event = tp.event_log.read(&tp.run_id, seq).unwrap().unwrap();
        assert_eq!(event.value.event_type, "");
    }

    #[test]
    fn test_eventlog_long_event_type() {
        let tp = TestPrimitives::new();
        let long_type = "event_".to_string() + &"a".repeat(1000);
        let version = tp
            .event_log
            .append(&tp.run_id, &long_type, values::null())
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };
        let event = tp.event_log.read(&tp.run_id, seq).unwrap().unwrap();
        assert_eq!(event.value.event_type, long_type);
    }

    #[test]
    fn test_statecell_empty_name() {
        let tp = TestPrimitives::new();
        tp.state_cell.init(&tp.run_id, "", values::int(0)).unwrap();
        assert!(tp.state_cell.read(&tp.run_id, "").unwrap().is_some());
    }

}

// =============================================================================
// Unicode and Special Characters
// =============================================================================

mod unicode_and_special {
    use super::*;

    #[test]
    fn test_kv_unicode_key() {
        let tp = TestPrimitives::new();
        let unicode_key = "キー_🔑_مفتاح";
        tp.kv.put(&tp.run_id, unicode_key, values::int(42)).unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, unicode_key).unwrap().map(|v| v.value),
            Some(values::int(42))
        );
    }

    #[test]
    fn test_kv_unicode_value() {
        let tp = TestPrimitives::new();
        let unicode_value = "值_🎉_قيمة_価値";
        tp.kv
            .put(&tp.run_id, "key", values::string(unicode_value))
            .unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value),
            Some(values::string(unicode_value))
        );
    }

    #[test]
    fn test_kv_emoji_key() {
        let tp = TestPrimitives::new();
        let emoji_key = "🔥💯🚀";
        tp.kv.put(&tp.run_id, emoji_key, values::int(100)).unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, emoji_key).unwrap().map(|v| v.value),
            Some(values::int(100))
        );
    }

    #[test]
    fn test_kv_newlines_in_string() {
        let tp = TestPrimitives::new();
        let multiline = "line1\nline2\r\nline3";
        tp.kv
            .put(&tp.run_id, "key", values::string(multiline))
            .unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value),
            Some(values::string(multiline))
        );
    }

    #[test]
    fn test_kv_null_bytes_in_key() {
        let tp = TestPrimitives::new();
        let key_with_null = "key\0with\0nulls";
        tp.kv
            .put(&tp.run_id, key_with_null, values::int(1))
            .unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, key_with_null).unwrap().map(|v| v.value),
            Some(values::int(1))
        );
    }

    #[test]
    fn test_kv_binary_data() {
        let tp = TestPrimitives::new();
        let binary: Vec<u8> = (0..=255).collect();
        tp.kv
            .put(&tp.run_id, "binary", values::bytes(&binary))
            .unwrap();
        assert_eq!(
            tp.kv.get(&tp.run_id, "binary").unwrap().map(|v| v.value),
            Some(values::bytes(&binary))
        );
    }

    #[test]
    fn test_eventlog_unicode_event_type() {
        let tp = TestPrimitives::new();
        let unicode_type = "イベント_🎯_حدث";
        let version = tp
            .event_log
            .append(&tp.run_id, unicode_type, values::null())
            .unwrap();
        let Version::Sequence(seq) = version else { panic!("Expected Sequence version") };
        let event = tp.event_log.read(&tp.run_id, seq).unwrap().unwrap();
        assert_eq!(event.value.event_type, unicode_type);
    }

    #[test]
    fn test_statecell_unicode_name() {
        let tp = TestPrimitives::new();
        let unicode_name = "状態_📊_حالة";
        tp.state_cell
            .init(&tp.run_id, unicode_name, values::int(0))
            .unwrap();
        assert!(tp
            .state_cell
            .read(&tp.run_id, unicode_name)
            .unwrap()
            .is_some());
    }

}

// =============================================================================
// Concurrent Edge Cases
// =============================================================================

mod concurrent_edge_cases {
    use super::*;

    #[test]
    fn test_many_concurrent_writes_same_key() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;
        let num_threads = 20;

        // Many threads writing to same key
        let results =
            concurrent::run_with_shared(num_threads, (tp.clone(), run_id), |i, (tp, run_id)| {
                tp.kv.put(run_id, "contended_key", values::int(i as i64))
            });

        // All writes should succeed
        for result in &results {
            assert!(result.is_ok());
        }

        // Final value is from one of the threads
        let final_value = tp.kv.get(&run_id, "contended_key").unwrap().unwrap().value;
        if let Value::Int(n) = final_value {
            assert!(n >= 0 && n < num_threads as i64);
        } else {
            panic!("Expected I64");
        }
    }

    #[test]
    fn test_concurrent_cas_exactly_one_winner() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;

        // Initialize cell
        tp.state_cell.init(&run_id, "cell", values::int(0)).unwrap();

        let num_threads = 10;

        // All threads try CAS with version 1
        // CAS returns Result<u64> (new version) on success, error on version mismatch
        let results =
            concurrent::run_with_shared(num_threads, (tp.clone(), run_id), |i, (tp, run_id)| {
                tp.state_cell.cas(run_id, "cell", 1, values::int(i as i64))
            });

        // Exactly one should succeed (get Ok with new version), others fail with version mismatch
        let winners: usize = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(winners, 1, "Expected exactly 1 CAS winner, got {}", winners);
    }

    #[test]
    fn test_concurrent_init_exactly_one_succeeds() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;
        let num_threads = 10;

        // All threads try to init same cell
        let results =
            concurrent::run_with_shared(num_threads, (tp.clone(), run_id), |i, (tp, run_id)| {
                tp.state_cell.init(run_id, "cell", values::int(i as i64))
            });

        // Exactly one should succeed
        let successes: usize = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(
            successes, 1,
            "Expected exactly 1 init success, got {}",
            successes
        );
    }

    #[test]
    fn test_concurrent_eventlog_appends_all_succeed() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;
        let num_threads = 10;

        // All threads append events
        let results =
            concurrent::run_with_shared(num_threads, (tp.clone(), run_id), |i, (tp, run_id)| {
                tp.event_log
                    .append(run_id, &format!("thread_{}", i), values::int(i as i64))
            });

        // All should succeed
        for result in &results {
            assert!(result.is_ok());
        }

        // All events present
        assert_eq!(tp.event_log.len(&run_id).unwrap(), num_threads as u64);
    }

    #[test]
    fn test_high_contention_increment() {
        let tp = Arc::new(TestPrimitives::new());
        let run_id = tp.run_id;

        // Initialize counter
        tp.state_cell
            .init(&run_id, "counter", values::int(0))
            .unwrap();

        let num_threads = 20;
        let increments_per_thread = 10;

        // Each thread increments via transition
        // transition() takes a closure that receives &State
        let results = concurrent::run_with_shared(
            num_threads,
            (tp.clone(), run_id),
            move |_, (tp, run_id)| {
                let mut successes = 0;
                for _ in 0..increments_per_thread {
                    let result = tp.state_cell.transition(run_id, "counter", |state| {
                        let current = if let Value::Int(n) = &state.value {
                            *n
                        } else {
                            0
                        };
                        Ok((values::int(current + 1), ()))
                    });
                    if result.is_ok() {
                        successes += 1;
                    }
                }
                successes
            },
        );

        // All increments should succeed
        let total_successes: i32 = results.iter().sum();
        assert_eq!(
            total_successes,
            (num_threads * increments_per_thread) as i32
        );

        // Final value should be total increments
        let state = tp.state_cell.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(
            state.value.value,
            values::int((num_threads * increments_per_thread) as i64)
        );
    }
}

// =============================================================================
// Delete Edge Cases
// =============================================================================

mod delete_edge_cases {
    use super::*;

    #[test]
    fn test_kv_delete_nonexistent() {
        let tp = TestPrimitives::new();
        // Should not error
        tp.kv.delete(&tp.run_id, "nonexistent").unwrap();
    }

    #[test]
    fn test_kv_delete_then_put() {
        let tp = TestPrimitives::new();
        tp.kv.put(&tp.run_id, "key", values::int(1)).unwrap();
        tp.kv.delete(&tp.run_id, "key").unwrap();
        tp.kv.put(&tp.run_id, "key", values::int(2)).unwrap();
        assert_eq!(tp.kv.get(&tp.run_id, "key").unwrap().map(|v| v.value), Some(values::int(2)));
    }

    #[test]
    fn test_statecell_delete_nonexistent() {
        let tp = TestPrimitives::new();
        // Should not error
        tp.state_cell.delete(&tp.run_id, "nonexistent").unwrap();
    }

    #[test]
    fn test_statecell_delete_then_init() {
        let tp = TestPrimitives::new();
        tp.state_cell
            .init(&tp.run_id, "cell", values::int(1))
            .unwrap();
        tp.state_cell.delete(&tp.run_id, "cell").unwrap();

        // Can init again after delete
        tp.state_cell
            .init(&tp.run_id, "cell", values::int(2))
            .unwrap();
        let state = tp.state_cell.read(&tp.run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(2));
        assert_eq!(state.value.version, 1); // Version resets after delete
    }
}
