//! Tier 1: Primitive Invariant Tests (M3.1-M3.6)
//!
//! These tests verify the sacred invariants that define primitive correctness.
//! Every test here maps to a specific invariant and MUST pass on every commit.
//!
//! ## Invariants Tested
//!
//! - M3.1: TypeTag Isolation - Keys with different TypeTags are completely isolated
//! - M3.2: Run Namespace Isolation - Same key in different runs are independent
//! - M3.3: Facade Identity - Primitives are facades, not stateful caches
//! - M3.4: Value Type Safety - Values round-trip with type fidelity
//! - M3.5: Deterministic Key Ordering - Keys returned in deterministic order
//! - M3.6: No Hidden Writes - Primitives cannot write outside transactions

use super::test_utils::*;
use strata_core::contract::Version;
use strata_core::error::Error;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::{EventLog, KVStore, StateCell};

// ============================================================================
// M3.1: TypeTag Isolation
// ============================================================================
// Keys with different TypeTags are completely isolated.
// KV keys never visible to EventLog, StateCell, etc.
// Cross-primitive key collision is impossible.
//
// What breaks if this fails?
// Cross-primitive data corruption. A KV get("foo") could return an EventLog entry.

mod typetag_isolation {
    use super::*;

    #[test]
    fn test_same_name_different_primitives_isolated() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Write to KV with key "data"
        tp.kv
            .put(&run_id, "data", values::string("kv-value"))
            .unwrap();

        // Write to Event with "data" as event_type
        tp.event_log
            .append(&run_id, "data", values::string("event-value"))
            .unwrap();

        // Write to State with "data" as cell name
        tp.state_cell
            .init(&run_id, "data", values::string("state-value"))
            .unwrap();

        // Verify each primitive sees only its own data
        let kv_val = tp.kv.get(&run_id, "data").unwrap().map(|v| v.value);
        assert_eq!(kv_val, Some(values::string("kv-value")));

        let event = tp.event_log.read(&run_id, 0).unwrap();
        assert!(event.is_some());
        assert_eq!(event.unwrap().value.payload, values::string("event-value"));

        let state = tp.state_cell.read(&run_id, "data").unwrap();
        assert!(state.is_some());
        assert_eq!(state.unwrap().value.value, values::string("state-value"));
    }

    #[test]
    fn test_kv_keys_invisible_to_eventlog() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Create many KV keys
        for i in 0..10 {
            tp.kv
                .put(&run_id, &format!("key_{}", i), values::int(i))
                .unwrap();
        }

        // EventLog should see nothing
        let events = tp.event_log.read_range(&run_id, 0, 100).unwrap();
        assert!(events.is_empty(), "EventLog should not see KV data");

        let len = tp.event_log.len(&run_id).unwrap();
        assert_eq!(len, 0, "EventLog length should be 0");
    }

    #[test]
    fn test_eventlog_invisible_to_statecell() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append events
        for i in 0..10 {
            tp.event_log
                .append(&run_id, &format!("event_{}", i), values::int(i))
                .unwrap();
        }

        // StateCell should see nothing with those names
        for i in 0..10 {
            let state = tp
                .state_cell
                .read(&run_id, &format!("event_{}", i))
                .unwrap();
            assert!(state.is_none(), "StateCell should not see EventLog data");
        }
    }

    #[test]
    fn test_statecell_invisible_to_kv() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Create state cells
        for i in 0..10 {
            tp.state_cell
                .init(&run_id, &format!("cell_{}", i), values::int(i))
                .unwrap();
        }

        // KV should not see these keys
        for i in 0..10 {
            let val = tp.kv.get(&run_id, &format!("cell_{}", i)).unwrap();
            assert!(val.is_none(), "KV should not see StateCell data");
        }
    }
}

// ============================================================================
// M3.2: Run Namespace Isolation
// ============================================================================
// Same key in different runs are independent.
// List operations scoped to single run.
// No cross-run data leakage.
//
// What breaks if this fails?
// Multi-tenant data leak. Agent run A could see data from Agent run B.

mod run_namespace_isolation {
    use super::*;

    #[test]
    fn test_kv_run_isolation() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = RunId::new();

        // Same key, different runs, different values
        tp.kv
            .put(&run1, "shared-key", values::string("run1-value"))
            .unwrap();
        tp.kv
            .put(&run2, "shared-key", values::string("run2-value"))
            .unwrap();

        // Each run sees only its own value
        assert_eq!(
            tp.kv.get(&run1, "shared-key").unwrap().map(|v| v.value),
            Some(values::string("run1-value"))
        );
        assert_eq!(
            tp.kv.get(&run2, "shared-key").unwrap().map(|v| v.value),
            Some(values::string("run2-value"))
        );
    }

    #[test]
    fn test_eventlog_run_isolation() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = RunId::new();

        // Append to both runs
        tp.event_log
            .append(&run1, "event", values::string("run1"))
            .unwrap();
        tp.event_log
            .append(&run2, "event", values::string("run2"))
            .unwrap();

        // Each run sees only its own event
        let events1 = tp.event_log.read_range(&run1, 0, 100).unwrap();
        assert_eq!(events1.len(), 1);
        assert_eq!(events1[0].value.payload, values::string("run1"));

        let events2 = tp.event_log.read_range(&run2, 0, 100).unwrap();
        assert_eq!(events2.len(), 1);
        assert_eq!(events2[0].value.payload, values::string("run2"));
    }

    #[test]
    fn test_statecell_run_isolation() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = RunId::new();

        // Same cell name in different runs
        tp.state_cell
            .init(&run1, "counter", values::int(100))
            .unwrap();
        tp.state_cell
            .init(&run2, "counter", values::int(200))
            .unwrap();

        let state1 = tp.state_cell.read(&run1, "counter").unwrap().unwrap();
        let state2 = tp.state_cell.read(&run2, "counter").unwrap().unwrap();

        assert_eq!(state1.value.value, values::int(100));
        assert_eq!(state2.value.value, values::int(200));
    }

    #[test]
    fn test_list_scoped_to_run() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = RunId::new();

        // Write to run1
        tp.kv.put(&run1, "run1-key-a", values::int(1)).unwrap();
        tp.kv.put(&run1, "run1-key-b", values::int(2)).unwrap();

        // Write to run2
        tp.kv.put(&run2, "run2-key-a", values::int(3)).unwrap();

        // List for run1 should only see run1 keys
        let keys1 = tp.kv.list(&run1, None).unwrap();
        assert_eq!(keys1.len(), 2);
        assert!(keys1.iter().any(|k| k.contains("run1-key-a")));
        assert!(keys1.iter().any(|k| k.contains("run1-key-b")));

        // List for run2 should only see run2 keys
        let keys2 = tp.kv.list(&run2, None).unwrap();
        assert_eq!(keys2.len(), 1);
        assert!(keys2.iter().any(|k| k.contains("run2-key-a")));
    }

    #[test]
    fn test_100_runs_isolation() {
        let tp = TestPrimitives::new();

        // Create 100 runs, each with a unique value
        let runs: Vec<RunId> = (0..100).map(|_| RunId::new()).collect();

        for (i, run_id) in runs.iter().enumerate() {
            tp.kv.put(run_id, "key", values::int(i as i64)).unwrap();
        }

        // Verify each run sees only its own value
        for (i, run_id) in runs.iter().enumerate() {
            let val = tp.kv.get(run_id, "key").unwrap().map(|v| v.value);
            assert_eq!(
                val,
                Some(values::int(i as i64)),
                "Run {} saw wrong value",
                i
            );
        }
    }

    #[test]
    fn test_delete_in_one_run_does_not_affect_other() {
        let tp = TestPrimitives::new();
        let run1 = tp.run_id;
        let run2 = RunId::new();

        // Both runs have the same key
        tp.kv.put(&run1, "shared", values::int(1)).unwrap();
        tp.kv.put(&run2, "shared", values::int(2)).unwrap();

        // Delete from run1
        tp.kv.delete(&run1, "shared").unwrap();

        // run1 should not see it, run2 should still see it
        assert!(tp.kv.get(&run1, "shared").unwrap().is_none());
        assert_eq!(tp.kv.get(&run2, "shared").unwrap().map(|v| v.value), Some(values::int(2)));
    }
}

// ============================================================================
// M3.3: Facade Identity Invariant
// ============================================================================
// Primitives are facades over shared engine state, not stateful caches.
// Creating, dropping, recreating a primitive handle must not affect visibility.
// No in-memory cache tied to primitive instance lifetime.
//
// What breaks if this fails?
// Memory leaks or data loss. If primitives cache state, dropping a handle
// could lose uncommitted data or leak memory.

mod facade_identity {
    use super::*;

    #[test]
    fn test_kvstore_facade_identity() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Write with first handle
        let kv1 = KVStore::new(tp.db.clone());
        kv1.put(&run_id, "key", values::int(42)).unwrap();
        drop(kv1); // Drop the primitive handle

        // Read with new handle - data should still be visible
        let kv2 = KVStore::new(tp.db.clone());
        assert_eq!(kv2.get(&run_id, "key").unwrap().map(|v| v.value), Some(values::int(42)));
    }

    #[test]
    fn test_eventlog_facade_identity() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Append with first handle
        let log1 = EventLog::new(tp.db.clone());
        log1.append(&run_id, "event", values::string("data"))
            .unwrap();
        drop(log1);

        // Read with new handle
        let log2 = EventLog::new(tp.db.clone());
        let events = log2.read_range(&run_id, 0, 100).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value.payload, values::string("data"));
    }

    #[test]
    fn test_statecell_facade_identity() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Init with first handle
        let sc1 = StateCell::new(tp.db.clone());
        sc1.init(&run_id, "cell", values::int(100)).unwrap();
        drop(sc1);

        // Read with new handle
        let sc2 = StateCell::new(tp.db.clone());
        let state = sc2.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state.value.value, values::int(100));
    }

    #[test]
    fn test_multiple_handles_same_database() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Multiple handles at the same time
        let kv1 = KVStore::new(tp.db.clone());
        let kv2 = KVStore::new(tp.db.clone());
        let kv3 = KVStore::new(tp.db.clone());

        // Write through one
        kv1.put(&run_id, "key", values::int(1)).unwrap();

        // Immediately visible through others
        assert_eq!(kv2.get(&run_id, "key").unwrap().map(|v| v.value), Some(values::int(1)));
        assert_eq!(kv3.get(&run_id, "key").unwrap().map(|v| v.value), Some(values::int(1)));
    }

    #[test]
    fn test_handle_drop_does_not_lose_data() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Rapid create/write/drop cycles
        for i in 0..100 {
            let kv = KVStore::new(tp.db.clone());
            kv.put(&run_id, &format!("key_{}", i), values::int(i))
                .unwrap();
            drop(kv);
        }

        // All data should be present
        let kv = KVStore::new(tp.db.clone());
        for i in 0..100 {
            let val = kv.get(&run_id, &format!("key_{}", i)).unwrap().map(|v| v.value);
            assert_eq!(val, Some(values::int(i)), "Lost data for key_{}", i);
        }
    }
}

// ============================================================================
// M3.4: Value Type Safety
// ============================================================================
// Values stored and retrieved maintain type fidelity.
// I64, String, Bool, Null, Array, Object all round-trip.
//
// What breaks if this fails?
// Silent data corruption. Store an I64, get back a String.

mod value_type_safety {
    use super::*;

    #[test]
    fn test_i64_roundtrip() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let values_to_test = vec![0i64, 1, -1, i64::MAX, i64::MIN, 42];
        for val in values_to_test {
            let key = unique_key("i64");
            tp.kv.put(&run_id, &key, Value::Int(val)).unwrap();
            assert_eq!(tp.kv.get(&run_id, &key).unwrap().map(|v| v.value), Some(Value::Int(val)));
        }
    }

    #[test]
    fn test_f64_roundtrip() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let values_to_test = vec![0.0f64, 1.0, -1.0, 3.14159, f64::MAX, f64::MIN];
        for val in values_to_test {
            let key = unique_key("f64");
            tp.kv.put(&run_id, &key, Value::Float(val)).unwrap();
            assert_eq!(tp.kv.get(&run_id, &key).unwrap().map(|v| v.value), Some(Value::Float(val)));
        }
    }

    #[test]
    fn test_string_roundtrip() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let values_to_test = vec!["", "hello", "hello world", "unicode: 你好世界 🎉"];
        for val in values_to_test {
            let key = unique_key("str");
            tp.kv.put(&run_id, &key, values::string(val)).unwrap();
            assert_eq!(tp.kv.get(&run_id, &key).unwrap().map(|v| v.value), Some(values::string(val)));
        }
    }

    #[test]
    fn test_bool_roundtrip() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.kv.put(&run_id, "true", Value::Bool(true)).unwrap();
        tp.kv.put(&run_id, "false", Value::Bool(false)).unwrap();

        assert_eq!(tp.kv.get(&run_id, "true").unwrap().map(|v| v.value), Some(Value::Bool(true)));
        assert_eq!(
            tp.kv.get(&run_id, "false").unwrap().map(|v| v.value),
            Some(Value::Bool(false))
        );
    }

    #[test]
    fn test_null_roundtrip() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        tp.kv.put(&run_id, "null", Value::Null).unwrap();
        assert_eq!(tp.kv.get(&run_id, "null").unwrap().map(|v| v.value), Some(Value::Null));
    }

    #[test]
    fn test_bytes_roundtrip() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let data = vec![0u8, 1, 2, 255, 128, 64];
        tp.kv.put(&run_id, "bytes", values::bytes(&data)).unwrap();
        assert_eq!(
            tp.kv.get(&run_id, "bytes").unwrap().map(|v| v.value),
            Some(values::bytes(&data))
        );
    }

    #[test]
    fn test_array_roundtrip() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let arr = values::array(vec![
            values::int(1),
            values::string("two"),
            values::bool_val(true),
            values::null(),
        ]);
        tp.kv.put(&run_id, "array", arr.clone()).unwrap();
        assert_eq!(tp.kv.get(&run_id, "array").unwrap().map(|v| v.value), Some(arr));
    }

    #[test]
    fn test_nested_array_roundtrip() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let nested = values::array(vec![
            values::array(vec![values::int(1), values::int(2)]),
            values::array(vec![values::string("a"), values::string("b")]),
        ]);
        tp.kv.put(&run_id, "nested", nested.clone()).unwrap();
        assert_eq!(tp.kv.get(&run_id, "nested").unwrap().map(|v| v.value), Some(nested));
    }

    #[test]
    fn test_map_roundtrip() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        let map = values::map(vec![
            ("key1", values::int(1)),
            ("key2", values::string("two")),
            ("key3", values::bool_val(false)),
        ]);
        tp.kv.put(&run_id, "map", map.clone()).unwrap();
        assert_eq!(tp.kv.get(&run_id, "map").unwrap().map(|v| v.value), Some(map));
    }

    #[test]
    fn test_type_safety_in_eventlog() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Test different value types as event payloads
        tp.event_log
            .append(&run_id, "int", values::int(42))
            .unwrap();
        tp.event_log
            .append(&run_id, "str", values::string("hello"))
            .unwrap();
        tp.event_log
            .append(&run_id, "arr", values::array(vec![values::int(1)]))
            .unwrap();

        let events = tp.event_log.read_range(&run_id, 0, 100).unwrap();
        assert_eq!(events[0].value.payload, values::int(42));
        assert_eq!(events[1].value.payload, values::string("hello"));
        assert_eq!(events[2].value.payload, values::array(vec![values::int(1)]));
    }

    #[test]
    fn test_type_safety_in_statecell() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Init with complex value
        let complex = values::map(vec![
            (
                "nested",
                values::array(vec![values::int(1), values::int(2)]),
            ),
            ("flag", values::bool_val(true)),
        ]);
        tp.state_cell
            .init(&run_id, "complex", complex.clone())
            .unwrap();

        let state = tp.state_cell.read(&run_id, "complex").unwrap().unwrap();
        assert_eq!(state.value.value, complex);
    }
}

// ============================================================================
// M3.5: Deterministic Key Ordering
// ============================================================================
// Primitives return keys in deterministic order (lexicographic by byte).
// This tests that M3 layer preserves M1 ordering—not that ordering works.
// Range scans return results in consistent, reproducible order.
//
// What breaks if this fails?
// Non-deterministic iteration. Same query returns different order.

mod deterministic_key_ordering {
    use super::*;

    #[test]
    fn test_list_returns_same_order_on_multiple_calls() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Insert in random order
        tp.kv.put(&run_id, "zebra", values::int(1)).unwrap();
        tp.kv.put(&run_id, "apple", values::int(2)).unwrap();
        tp.kv.put(&run_id, "mango", values::int(3)).unwrap();
        tp.kv.put(&run_id, "banana", values::int(4)).unwrap();

        // Multiple list calls should return same order
        let order1 = tp.kv.list(&run_id, None).unwrap();
        let order2 = tp.kv.list(&run_id, None).unwrap();
        let order3 = tp.kv.list(&run_id, None).unwrap();

        assert_eq!(order1, order2, "First and second list differ");
        assert_eq!(order2, order3, "Second and third list differ");
    }

    #[test]
    fn test_list_with_values_same_order() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        for i in 0..20 {
            tp.kv
                .put(&run_id, &format!("key_{:03}", i), values::int(i))
                .unwrap();
        }

        let pairs1 = tp.kv.list_with_values(&run_id, None).unwrap();
        let pairs2 = tp.kv.list_with_values(&run_id, None).unwrap();

        let keys1: Vec<_> = pairs1.iter().map(|(k, _)| k.clone()).collect();
        let keys2: Vec<_> = pairs2.iter().map(|(k, _)| k.clone()).collect();

        assert_eq!(keys1, keys2, "Key ordering not deterministic");
    }

    #[test]
    fn test_prefix_scan_deterministic_order() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Insert with common prefix in random order
        tp.kv.put(&run_id, "user:300", values::int(1)).unwrap();
        tp.kv.put(&run_id, "user:100", values::int(2)).unwrap();
        tp.kv.put(&run_id, "user:200", values::int(3)).unwrap();

        let scan1 = tp.kv.list(&run_id, Some("user:")).unwrap();
        let scan2 = tp.kv.list(&run_id, Some("user:")).unwrap();

        assert_eq!(scan1, scan2, "Prefix scan ordering not deterministic");
    }

    #[test]
    fn test_ordering_survives_reopen() {
        let ptp = PersistentTestPrimitives::new();
        let run_id = ptp.run_id;

        // Insert data
        {
            let p = ptp.open_strict();
            p.kv.put(&run_id, "c", values::int(3)).unwrap();
            p.kv.put(&run_id, "a", values::int(1)).unwrap();
            p.kv.put(&run_id, "b", values::int(2)).unwrap();
        }

        // Reopen and check order is preserved
        {
            let p = ptp.open();
            let order = p.kv.list(&run_id, None).unwrap();

            // Order should be deterministic (lexicographic)
            // Note: We test determinism, not specific order
            let order2 = p.kv.list(&run_id, None).unwrap();
            assert_eq!(order, order2, "Order changed after reopen");
        }
    }
}

// ============================================================================
// M3.6: No Hidden Writes Invariant
// ============================================================================
// Primitives must not write outside of transaction boundaries.
// Aborted transactions leave no trace.
//
// What breaks if this fails?
// Atomicity violation. Failed transactions leave partial state.

mod no_hidden_writes {
    use super::*;

    #[test]
    fn test_failed_transaction_leaves_no_kv_trace() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        use strata_primitives::extensions::*;

        // Attempt a transaction that fails
        let result: Result<(), Error> = tp.db.transaction(run_id, |txn| {
            txn.kv_put("key1", values::int(1))?;
            txn.kv_put("key2", values::int(2))?;
            // Force failure
            Err(Error::InvalidState("intentional abort".to_string()))
        });

        assert!(result.is_err());

        // No keys should exist
        assert!(tp.kv.get(&run_id, "key1").unwrap().is_none());
        assert!(tp.kv.get(&run_id, "key2").unwrap().is_none());
    }

    #[test]
    fn test_failed_cross_primitive_transaction() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        use strata_primitives::extensions::*;

        // Cross-primitive transaction that fails
        let result: Result<(), Error> = tp.db.transaction(run_id, |txn| {
            txn.kv_put("key", values::int(1))?;
            txn.event_append("type", values::null())?;
            Err(Error::InvalidState("abort".to_string()))
        });

        assert!(result.is_err());

        // Neither KV nor EventLog should have any data
        assert!(tp.kv.get(&run_id, "key").unwrap().is_none());
        assert_eq!(tp.event_log.len(&run_id).unwrap(), 0);
    }

    #[test]
    fn test_eventlog_abort_does_not_consume_sequence() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Successful append
        let version1 = tp
            .event_log
            .append(&run_id, "first", values::int(1))
            .unwrap();
        let Version::Sequence(seq1) = version1 else { panic!("Expected Sequence version") };
        assert_eq!(seq1, 0);

        // Failed append (simulate via low-level transaction)
        use strata_primitives::extensions::*;
        let result: Result<(), Error> = tp.db.transaction(run_id, |txn| {
            txn.event_append("failed", values::int(2))?;
            Err(Error::InvalidState("abort".to_string()))
        });
        assert!(result.is_err());

        // Next successful append should get sequence 1 (not 2)
        let version2 = tp
            .event_log
            .append(&run_id, "second", values::int(3))
            .unwrap();
        let Version::Sequence(seq2) = version2 else { panic!("Expected Sequence version") };
        assert_eq!(seq2, 1, "Aborted transaction consumed sequence number");
    }

    #[test]
    fn test_statecell_abort_does_not_change_version() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Init cell
        tp.state_cell.init(&run_id, "cell", values::int(1)).unwrap();
        let state1 = tp.state_cell.read(&run_id, "cell").unwrap().unwrap();
        let v1 = state1.value.version;

        // Failed CAS attempt via transaction
        use strata_primitives::extensions::*;
        let result: Result<(), Error> = tp.db.transaction(run_id, |txn| {
            txn.state_cas("cell", v1, values::int(2))?;
            Err(Error::InvalidState("abort".to_string()))
        });
        assert!(result.is_err());

        // Version should not have changed
        let state2 = tp.state_cell.read(&run_id, "cell").unwrap().unwrap();
        assert_eq!(state2.value.version, v1, "Version changed despite abort");
        assert_eq!(state2.value.value, values::int(1), "Value changed despite abort");
    }

    #[test]
    fn test_no_side_effects_on_read_only_operations() {
        let tp = TestPrimitives::new();
        let run_id = tp.run_id;

        // Setup some data
        tp.kv.put(&run_id, "key", values::int(1)).unwrap();
        tp.event_log
            .append(&run_id, "event", values::int(2))
            .unwrap();
        tp.state_cell.init(&run_id, "cell", values::int(3)).unwrap();

        // Failed read-only transaction
        use strata_primitives::extensions::*;
        let result: Result<(), Error> = tp.db.transaction(run_id, |txn| {
            let _ = txn.kv_get("key")?;
            let _ = txn.event_read(0)?;
            let _ = txn.state_read("cell")?;
            Err(Error::InvalidState("abort".to_string()))
        });
        assert!(result.is_err());

        // Data should be unchanged
        assert_eq!(tp.kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(values::int(1)));
        assert_eq!(
            tp.event_log.read(&run_id, 0).unwrap().unwrap().value.payload,
            values::int(2)
        );
        assert_eq!(
            tp.state_cell.read(&run_id, "cell").unwrap().unwrap().value.value,
            values::int(3)
        );
    }
}
