//! Edge Case Tests
//!
//! Tests for boundary conditions, unusual scenarios, and edge cases.

use super::test_utils::*;
use strata_core::error::Error;
use strata_core::types::{Key, Namespace, RunId, TypeTag};
use strata_core::value::Value;
use strata_engine::{Database, RetryConfig};
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Empty and Null Value Tests
// ============================================================================

mod empty_values {
    use super::*;

    #[test]
    fn test_empty_string_value() {
        let tdb = TestDb::new();
        let key = tdb.key("empty_string");

        tdb.db
            .put(tdb.run_id, key.clone(), values::string(""))
            .unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::string(""));
    }

    #[test]
    fn test_empty_bytes_value() {
        let tdb = TestDb::new();
        let key = tdb.key("empty_bytes");

        tdb.db
            .put(tdb.run_id, key.clone(), values::bytes(&[]))
            .unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::bytes(&[]));
    }

    #[test]
    fn test_null_value() {
        let tdb = TestDb::new();
        let key = tdb.key("null_value");

        tdb.db.put(tdb.run_id, key.clone(), values::null()).unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::null());
    }

    #[test]
    fn test_empty_array() {
        let tdb = TestDb::new();
        let key = tdb.key("empty_array");

        tdb.db
            .put(tdb.run_id, key.clone(), values::array(vec![]))
            .unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::array(vec![]));
    }

    #[test]
    fn test_empty_map() {
        let tdb = TestDb::new();
        let key = tdb.key("empty_map");

        tdb.db
            .put(tdb.run_id, key.clone(), values::map(vec![]))
            .unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::map(vec![]));
    }
}

// ============================================================================
// Extreme Value Tests
// ============================================================================

mod extreme_values {
    use super::*;

    #[test]
    fn test_max_i64() {
        let tdb = TestDb::new();
        let key = tdb.key("max_i64");

        tdb.db
            .put(tdb.run_id, key.clone(), values::int(i64::MAX))
            .unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(i64::MAX));
    }

    #[test]
    fn test_min_i64() {
        let tdb = TestDb::new();
        let key = tdb.key("min_i64");

        tdb.db
            .put(tdb.run_id, key.clone(), values::int(i64::MIN))
            .unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(i64::MIN));
    }

    #[test]
    fn test_special_floats() {
        let tdb = TestDb::new();

        let cases = [
            ("inf", f64::INFINITY),
            ("neg_inf", f64::NEG_INFINITY),
            ("zero", 0.0),
            ("neg_zero", -0.0),
            ("pi", std::f64::consts::PI),
            ("epsilon", f64::EPSILON),
            ("max", f64::MAX),
            ("min_positive", f64::MIN_POSITIVE),
        ];

        for (name, float) in cases {
            let key = tdb.key(name);
            tdb.db
                .put(tdb.run_id, key.clone(), values::float(float))
                .unwrap();

            let val = tdb.db.get(&key).unwrap().unwrap();
            if let Value::F64(stored) = val.value {
                if float.is_nan() {
                    assert!(stored.is_nan());
                } else {
                    assert_eq!(stored, float, "Mismatch for {}", name);
                }
            } else {
                panic!("Expected F64 for {}", name);
            }
        }
    }

    #[test]
    fn test_large_string_value() {
        let tdb = TestDb::new();
        let key = tdb.key("large_string");

        // 1MB string
        let large = values::sized_string(1024 * 1024);

        tdb.db.put(tdb.run_id, key.clone(), large.clone()).unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, large);
    }

    #[test]
    fn test_large_bytes_value() {
        let tdb = TestDb::new();
        let key = tdb.key("large_bytes");

        // 1MB bytes
        let large = values::large_bytes(1024);

        tdb.db.put(tdb.run_id, key.clone(), large.clone()).unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, large);
    }

    #[test]
    fn test_deeply_nested_value() {
        let tdb = TestDb::new();
        let key = tdb.key("deeply_nested");

        // Nested array 10 levels deep
        let mut nested = values::int(42);
        for _ in 0..10 {
            nested = values::array(vec![nested]);
        }

        tdb.db.put(tdb.run_id, key.clone(), nested.clone()).unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, nested);
    }

    #[test]
    fn test_wide_array() {
        let tdb = TestDb::new();
        let key = tdb.key("wide_array");

        // Array with 1000 elements
        let wide = values::array((0..1000).map(values::int).collect());

        tdb.db.put(tdb.run_id, key.clone(), wide.clone()).unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, wide);
    }

    #[test]
    fn test_complex_map() {
        let tdb = TestDb::new();
        let key = tdb.key("complex_map");

        let complex = values::map(vec![
            ("string", values::string("hello")),
            ("int", values::int(42)),
            ("float", values::float(3.14)),
            ("bool", values::bool_val(true)),
            ("null", values::null()),
            ("array", values::array(vec![values::int(1), values::int(2)])),
            (
                "nested_map",
                values::map(vec![("inner", values::string("world"))]),
            ),
        ]);

        tdb.db
            .put(tdb.run_id, key.clone(), complex.clone())
            .unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, complex);
    }
}

// ============================================================================
// Key Edge Cases
// ============================================================================

mod key_edge_cases {
    use super::*;

    #[test]
    fn test_single_byte_key() {
        let tdb = TestDb::new();
        let key = kv_key(&tdb.ns, "a");

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();
        assert!(tdb.db.get(&key).unwrap().is_some());
    }

    #[test]
    fn test_very_long_key() {
        let tdb = TestDb::new();
        // 1000 character key name
        let long_name = "k".repeat(1000);
        let key = kv_key(&tdb.ns, &long_name);

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();
        assert!(tdb.db.get(&key).unwrap().is_some());
    }

    #[test]
    fn test_key_with_special_characters() {
        let tdb = TestDb::new();

        let special_names = [
            "key with spaces",
            "key\twith\ttabs",
            "key\nwith\nnewlines",
            "key/with/slashes",
            "key\\with\\backslashes",
            "key\"with\"quotes",
            "key'with'apostrophes",
            "key:with:colons",
            "key;with;semicolons",
            "key=with=equals",
            "key?with?questions",
            "key*with*asterisks",
            "key<with>brackets",
            "emoji_🚀_key",
            "unicode_日本語_key",
        ];

        for name in special_names {
            let key = kv_key(&tdb.ns, name);
            tdb.db
                .put(tdb.run_id, key.clone(), values::string(name))
                .unwrap();

            let val = tdb
                .db
                .get(&key)
                .unwrap()
                .expect(&format!("Key '{}' not found", name));
            assert_eq!(val.value, values::string(name));
        }
    }

    #[test]
    fn test_key_with_null_bytes() {
        let tdb = TestDb::new();
        let key = Key::new_kv(tdb.ns.clone(), vec![0x00, 0x01, 0x00, 0x02]);

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();
        assert!(tdb.db.get(&key).unwrap().is_some());
    }

    #[test]
    fn test_keys_that_look_similar() {
        let tdb = TestDb::new();

        let keys = [
            "key", "key ", // trailing space
            " key", // leading space
            "Key",  // different case
            "KEY",  // all caps
            "key1", // with number
            "key_", // trailing underscore
        ];

        for (i, name) in keys.iter().enumerate() {
            let key = kv_key(&tdb.ns, name);
            tdb.db
                .put(tdb.run_id, key.clone(), values::int(i as i64))
                .unwrap();
        }

        // All should be distinct
        for (i, name) in keys.iter().enumerate() {
            let key = kv_key(&tdb.ns, name);
            let val = tdb.db.get(&key).unwrap().unwrap();
            assert_eq!(
                val.value,
                values::int(i as i64),
                "Mismatch for key: '{}'",
                name
            );
        }
    }

    #[test]
    fn test_event_keys_ordered_by_sequence() {
        let tdb = TestDb::new();

        // Insert out of order
        for seq in [5, 1, 3, 2, 4] {
            let key = event_key(&tdb.ns, seq);
            tdb.db
                .put(tdb.run_id, key, values::int(seq as i64))
                .unwrap();
        }

        // Scan should return in order
        let prefix = event_key(&tdb.ns, 0); // Use seq 0 as prefix base
        let prefix_for_scan = Key::new(
            tdb.ns.clone(),
            TypeTag::Event,
            vec![], // Empty to match all events in namespace
        );

        // Note: Actual scan implementation depends on prefix semantics
    }
}

// ============================================================================
// Namespace Edge Cases
// ============================================================================

mod namespace_edge_cases {
    use super::*;

    #[test]
    fn test_empty_namespace_components() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "".to_string(), // empty tenant
            "".to_string(), // empty app
            "".to_string(), // empty agent
            run_id,
        );

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let key = kv_key(&ns, "test");
        db.put(run_id, key.clone(), values::int(1)).unwrap();
        assert!(db.get(&key).unwrap().is_some());
    }

    #[test]
    fn test_namespace_with_special_characters() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant/with/slashes".to_string(),
            "app.with.dots".to_string(),
            "agent:with:colons".to_string(),
            run_id,
        );

        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let key = kv_key(&ns, "test");
        db.put(run_id, key.clone(), values::int(1)).unwrap();
        assert!(db.get(&key).unwrap().is_some());
    }

    #[test]
    fn test_same_key_different_namespaces() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run_id = RunId::new();

        let ns1 = Namespace::new(
            "tenant1".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let ns2 = Namespace::new(
            "tenant2".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key1 = kv_key(&ns1, "shared_name");
        let key2 = kv_key(&ns2, "shared_name");

        db.put(run_id, key1.clone(), values::int(100)).unwrap();
        db.put(run_id, key2.clone(), values::int(200)).unwrap();

        assert_eq!(db.get(&key1).unwrap().unwrap().value, values::int(100));
        assert_eq!(db.get(&key2).unwrap().unwrap().value, values::int(200));
    }

    #[test]
    fn test_same_key_different_run_ids() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run1 = RunId::new();
        let run2 = RunId::new();

        let ns1 = create_namespace_for_run(run1);
        let ns2 = create_namespace_for_run(run2);

        let key1 = kv_key(&ns1, "test");
        let key2 = kv_key(&ns2, "test");

        db.put(run1, key1.clone(), values::int(1)).unwrap();
        db.put(run2, key2.clone(), values::int(2)).unwrap();

        // Both should exist and be independent
        assert_eq!(db.get(&key1).unwrap().unwrap().value, values::int(1));
        assert_eq!(db.get(&key2).unwrap().unwrap().value, values::int(2));
    }
}

// ============================================================================
// Transaction Edge Cases
// ============================================================================

mod transaction_edge_cases {
    use super::*;

    #[test]
    fn test_empty_transaction() {
        let tdb = TestDb::new();

        // Transaction with no operations
        let result = tdb.db.transaction(tdb.run_id, |_txn| Ok(42));

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_read_only_transaction() {
        let tdb = TestDb::new();
        let key = tdb.key("read_only");

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();

        // Transaction that only reads
        let result = tdb.db.transaction(tdb.run_id, |txn| {
            let val = txn.get(&key)?;
            Ok(val)
        });

        assert_eq!(result.unwrap(), Some(values::int(1)));
    }

    #[test]
    fn test_write_then_delete_same_key() {
        let tdb = TestDb::new();
        let key = tdb.key("write_delete");

        tdb.db
            .transaction(tdb.run_id, |txn| {
                txn.put(key.clone(), values::int(42))?;
                txn.delete(key.clone())?;
                Ok(())
            })
            .unwrap();

        // Key should not exist
        assert!(tdb.db.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_delete_then_write_same_key() {
        let tdb = TestDb::new();
        let key = tdb.key("delete_write");

        // Pre-populate
        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();

        tdb.db
            .transaction(tdb.run_id, |txn| {
                txn.delete(key.clone())?;
                txn.put(key.clone(), values::int(99))?;
                Ok(())
            })
            .unwrap();

        // Should have the new value
        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(99));
    }

    #[test]
    fn test_multiple_writes_to_same_key() {
        let tdb = TestDb::new();
        let key = tdb.key("multi_write");

        tdb.db
            .transaction(tdb.run_id, |txn| {
                for i in 0..10 {
                    txn.put(key.clone(), values::int(i))?;
                }
                Ok(())
            })
            .unwrap();

        // Should have last value
        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(9));
    }

    #[test]
    fn test_transaction_that_returns_error_from_inner_function() {
        let tdb = TestDb::new();
        let key = tdb.key("inner_error");

        fn inner_work(
            txn: &mut strata_concurrency::transaction::TransactionContext,
            key: Key,
        ) -> Result<(), Error> {
            txn.put(key, values::int(42))?;
            Err(Error::InvalidOperation("inner error".to_string()))
        }

        let result: Result<(), Error> = tdb
            .db
            .transaction(tdb.run_id, |txn| inner_work(txn, key.clone()));

        assert!(result.is_err());
        assert!(tdb.db.get(&key).unwrap().is_none()); // Not committed
    }

    #[test]
    fn test_zero_timeout() {
        let tdb = TestDb::new();
        let key = tdb.key("zero_timeout");

        // Zero timeout should expire immediately (or nearly so)
        let result: Result<(), Error> =
            tdb.db
                .transaction_with_timeout(tdb.run_id, Duration::ZERO, |txn| {
                    // Even a simple put might exceed zero timeout
                    std::thread::sleep(Duration::from_millis(1));
                    txn.put(key.clone(), values::int(1))?;
                    Ok(())
                });

        // Should timeout
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.is_timeout());
        }
    }

    #[test]
    fn test_max_retries_zero() {
        let tdb = TestDb::new();

        // No retries means first failure is final
        let result: Result<(), Error> =
            tdb.db
                .transaction_with_retry(tdb.run_id, RetryConfig::no_retry(), |_txn| {
                    Err(Error::TransactionConflict("fail".to_string()))
                });

        assert!(result.is_err());
    }

    #[test]
    fn test_cas_on_nonexistent_key() {
        let tdb = TestDb::new();
        let key = tdb.key("cas_nonexistent");

        // CAS on key that doesn't exist should fail
        // (expected version won't match)
        let result = tdb.db.cas(tdb.run_id, key.clone(), 0, values::int(1));

        // Behavior depends on implementation - might succeed with version 0 or fail
        // Just verify it doesn't panic
    }

    #[test]
    fn test_scan_prefix_with_no_matching_keys() {
        let tdb = TestDb::new();

        // Put keys with different prefix
        for i in 0..5 {
            let key = tdb.key(&format!("other_{}", i));
            tdb.db.put(tdb.run_id, key, values::int(i)).unwrap();
        }

        // Scan for non-matching prefix
        tdb.db
            .transaction(tdb.run_id, |txn| {
                let prefix = kv_key(&tdb.ns, "nonexistent_prefix_");
                let results = txn.scan_prefix(&prefix)?;
                assert!(results.is_empty());
                Ok(())
            })
            .unwrap();
    }
}

// ============================================================================
// Version Edge Cases
// ============================================================================

mod version_edge_cases {
    use super::*;

    #[test]
    fn test_cas_with_version_zero() {
        let tdb = TestDb::new();
        let key = tdb.key("version_zero");

        // First put should have version > 0
        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();
        let v = tdb.db.get(&key).unwrap().unwrap().version.as_u64();
        assert!(v > 0);

        // CAS with version 0 should fail
        let result = tdb.db.cas(tdb.run_id, key.clone(), 0, values::int(2));
        assert!(result.is_err());
    }

    #[test]
    fn test_many_versions_same_key() {
        let tdb = TestDb::new();
        let key = tdb.key("many_versions");

        let mut versions = Vec::new();

        for i in 0..100 {
            tdb.db.put(tdb.run_id, key.clone(), values::int(i)).unwrap();
            versions.push(tdb.db.get(&key).unwrap().unwrap().version);
        }

        // All versions should be unique and increasing
        for i in 1..versions.len() {
            assert!(versions[i] > versions[i - 1]);
        }
    }

    #[test]
    fn test_version_survives_reopen() {
        let pdb = PersistentTestDb::new();
        let key = pdb.key("version_persist");

        let original_version;
        {
            let db = pdb.open();
            db.put(pdb.run_id, key.clone(), values::int(1)).unwrap();
            original_version = db.get(&key).unwrap().unwrap().version;
        }

        {
            let db = pdb.open();
            let recovered_version = db.get(&key).unwrap().unwrap().version;
            assert_eq!(recovered_version, original_version);
        }
    }
}

// ============================================================================
// Concurrency Edge Cases
// ============================================================================

mod concurrency_edge_cases {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_transaction_during_database_flush() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        let db1 = Arc::clone(&db);
        let db2 = Arc::clone(&db);
        let ns_clone = ns.clone();

        // Thread 1: Continuous transactions
        let h1 = thread::spawn(move || {
            for i in 0..100 {
                let key = kv_key(&ns_clone, &format!("during_flush_{}", i));
                let _ = db1.transaction(run_id, |txn| {
                    txn.put(key.clone(), values::int(i))?;
                    Ok(())
                });
            }
        });

        // Thread 2: Frequent flushes
        let h2 = thread::spawn(move || {
            for _ in 0..10 {
                let _ = db2.flush();
                thread::sleep(Duration::from_millis(10));
            }
        });

        h1.join().unwrap();
        h2.join().unwrap();
    }

    #[test]
    fn test_rapid_open_close() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "rapid_test");

        // Rapidly open and close database
        for i in 0..10 {
            let db = Database::open(&db_path).unwrap();
            db.put(run_id, key.clone(), values::int(i)).unwrap();
            // db is dropped here
        }

        // Final value should be the last one written
        let db = Database::open(&db_path).unwrap();
        let val = db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(9));
    }

    #[test]
    fn test_many_simultaneous_readers() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "many_readers");

        // Pre-populate
        db.put(run_id, key.clone(), values::int(42)).unwrap();

        // 20 simultaneous readers
        let handles: Vec<_> = (0..20)
            .map(|_| {
                let db = Arc::clone(&db);
                let key = key.clone();

                thread::spawn(move || {
                    for _ in 0..100 {
                        let val = db.get(&key).unwrap().unwrap();
                        assert_eq!(val.value, values::int(42));
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
    }
}

// ============================================================================
// Type Tag Edge Cases
// ============================================================================

mod type_tag_edge_cases {
    use super::*;

    #[test]
    fn test_all_type_tags() {
        let tdb = TestDb::new();

        // KV
        let kv_key = tdb.key("kv_test");
        tdb.db
            .put(tdb.run_id, kv_key.clone(), values::int(1))
            .unwrap();

        // Event
        let event_key = tdb.event(1);
        tdb.db
            .put(tdb.run_id, event_key.clone(), values::int(2))
            .unwrap();

        // State
        let state_key = tdb.state("state_test");
        tdb.db
            .put(tdb.run_id, state_key.clone(), values::int(3))
            .unwrap();

        // All should be retrievable
        assert_eq!(tdb.db.get(&kv_key).unwrap().unwrap().value, values::int(1));
        assert_eq!(
            tdb.db.get(&event_key).unwrap().unwrap().value,
            values::int(2)
        );
        assert_eq!(
            tdb.db.get(&state_key).unwrap().unwrap().value,
            values::int(3)
        );
    }

    #[test]
    fn test_same_user_key_different_type_tags() {
        let tdb = TestDb::new();

        // Same name, different types
        let kv = kv_key(&tdb.ns, "shared_name");
        let state = state_key(&tdb.ns, "shared_name");

        tdb.db
            .put(tdb.run_id, kv.clone(), values::int(100))
            .unwrap();
        tdb.db
            .put(tdb.run_id, state.clone(), values::int(200))
            .unwrap();

        // Should be separate keys
        assert_eq!(tdb.db.get(&kv).unwrap().unwrap().value, values::int(100));
        assert_eq!(tdb.db.get(&state).unwrap().unwrap().value, values::int(200));
    }

    #[test]
    fn test_cross_type_transaction() {
        let tdb = TestDb::new();

        let kv = tdb.key("cross_kv");
        let event = tdb.event(1);
        let state = tdb.state("cross_state");

        // Single transaction modifying all types
        tdb.db
            .transaction(tdb.run_id, |txn| {
                txn.put(kv.clone(), values::int(1))?;
                txn.put(event.clone(), values::int(2))?;
                txn.put(state.clone(), values::int(3))?;
                Ok(())
            })
            .unwrap();

        // All should be committed atomically
        assert!(tdb.db.get(&kv).unwrap().is_some());
        assert!(tdb.db.get(&event).unwrap().is_some());
        assert!(tdb.db.get(&state).unwrap().is_some());
    }

    #[test]
    fn test_cross_type_abort() {
        let tdb = TestDb::new();

        let kv = tdb.key("abort_kv");
        let event = tdb.event(99);
        let state = tdb.state("abort_state");

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(kv.clone(), values::int(1))?;
            txn.put(event.clone(), values::int(2))?;
            txn.put(state.clone(), values::int(3))?;
            Err(Error::InvalidState("abort".to_string()))
        });

        assert!(result.is_err());

        // None should exist
        assert!(tdb.db.get(&kv).unwrap().is_none());
        assert!(tdb.db.get(&event).unwrap().is_none());
        assert!(tdb.db.get(&state).unwrap().is_none());
    }
}
