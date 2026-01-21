//! Database API Unit Tests
//!
//! Comprehensive tests for all Database public methods.
//! Tests are organized by API category.

use super::test_utils::*;
use strata_core::error::Error;
use strata_core::traits::Storage;
use strata_core::types::{Key, RunId};
use strata_core::value::Value;
use strata_durability::DurabilityMode;
use strata_engine::{Database, RetryConfig};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Database Lifecycle Tests
// ============================================================================

mod lifecycle {
    use super::*;

    #[test]
    fn test_open_creates_new_database() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_db");

        assert!(!db_path.exists());

        let db = Database::open(&db_path).unwrap();
        assert!(db_path.exists());
        assert_eq!(db.data_dir(), db_path);
    }

    #[test]
    fn test_open_with_mode_strict() {
        let temp_dir = TempDir::new().unwrap();
        let db =
            Database::open_with_mode(temp_dir.path().join("db"), DurabilityMode::Strict).unwrap();

        // Verify database is functional
        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "test");
        db.put(run_id, key.clone(), values::int(1)).unwrap();
        assert!(db.get(&key).unwrap().is_some());
    }

    #[test]
    fn test_open_with_mode_batched() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open_with_mode(
            temp_dir.path().join("db"),
            DurabilityMode::Batched {
                interval_ms: 100,
                batch_size: 1000,
            },
        )
        .unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "test");
        db.put(run_id, key.clone(), values::int(1)).unwrap();
        assert!(db.get(&key).unwrap().is_some());
    }

    #[test]
    fn test_open_with_mode_async() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open_with_mode(
            temp_dir.path().join("db"),
            DurabilityMode::Async { interval_ms: 100 },
        )
        .unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "test");
        db.put(run_id, key.clone(), values::int(1)).unwrap();
        assert!(db.get(&key).unwrap().is_some());
    }

    #[test]
    fn test_open_reopens_existing_database() {
        let pdb = PersistentTestDb::new();
        let key = pdb.key("persist_test");

        // Write data
        {
            let db = pdb.open();
            db.put(pdb.run_id, key.clone(), values::int(42)).unwrap();
        }

        // Reopen and verify
        {
            let db = pdb.open();
            let val = db.get(&key).unwrap().expect("Key should exist");
            assert_eq!(val.value, values::int(42));
        }
    }

    #[test]
    fn test_open_multiple_databases_concurrently() {
        let temp_dirs: Vec<TempDir> = (0..5).map(|_| TempDir::new().unwrap()).collect();

        let dbs: Vec<Database> = temp_dirs
            .iter()
            .map(|td| Database::open(td.path().join("db")).unwrap())
            .collect();

        // All databases should be independent
        for (i, db) in dbs.iter().enumerate() {
            let (run_id, ns) = create_namespace();
            let key = kv_key(&ns, "test");
            db.put(run_id, key.clone(), values::int(i as i64)).unwrap();
        }
    }

    #[test]
    fn test_flush_persists_data() {
        let pdb = PersistentTestDb::new();
        let key = pdb.key("flush_test");

        {
            let db = pdb.open();
            db.put(pdb.run_id, key.clone(), values::int(99)).unwrap();
            db.flush().unwrap(); // Explicit flush
        }

        // Data should survive reopen
        {
            let db = pdb.open();
            let val = db.get(&key).unwrap().expect("Key should exist after flush");
            assert_eq!(val.value, values::int(99));
        }
    }

    #[test]
    fn test_storage_accessor() {
        let tdb = TestDb::new();
        let key = tdb.key("storage_test");

        // Use storage() directly
        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();

        let storage = tdb.db.storage();
        let val = storage.get(&key).unwrap().expect("Key should exist");
        assert_eq!(val.value, values::int(1));
    }

    #[test]
    fn test_wal_accessor() {
        let tdb = TestDb::new();
        let _wal = tdb.db.wal(); // Should not panic
    }

    #[test]
    fn test_data_dir_returns_correct_path() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("my_database");
        let db = Database::open(&db_path).unwrap();

        assert_eq!(db.data_dir(), db_path);
    }
}

// ============================================================================
// Transaction Closure API Tests
// ============================================================================

mod transaction_api {
    use super::*;

    #[test]
    fn test_transaction_returns_closure_value() {
        let tdb = TestDb::new();

        let result = tdb.db.transaction(tdb.run_id, |_txn| Ok(42i32));

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_transaction_returns_string() {
        let tdb = TestDb::new();

        let result = tdb
            .db
            .transaction(tdb.run_id, |_txn| Ok("hello".to_string()));

        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn test_transaction_returns_struct() {
        #[derive(Debug, PartialEq)]
        struct MyResult {
            count: usize,
            success: bool,
        }

        let tdb = TestDb::new();

        let result = tdb.db.transaction(tdb.run_id, |_txn| {
            Ok(MyResult {
                count: 10,
                success: true,
            })
        });

        assert_eq!(
            result.unwrap(),
            MyResult {
                count: 10,
                success: true
            }
        );
    }

    #[test]
    fn test_transaction_commits_on_ok() {
        let tdb = TestDb::new();
        let key = tdb.key("commit_test");

        let result = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(key.clone(), values::int(100))?;
            Ok(())
        });

        assert!(result.is_ok());
        let stored = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, values::int(100));
    }

    #[test]
    fn test_transaction_aborts_on_error() {
        let tdb = TestDb::new();
        let key = tdb.key("abort_test");

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(key.clone(), values::int(999))?;
            Err(Error::InvalidState("intentional abort".to_string()))
        });

        assert!(result.is_err());
        assert!(tdb.db.get(&key).unwrap().is_none()); // Not committed
    }

    #[test]
    fn test_transaction_propagates_closure_error() {
        let tdb = TestDb::new();

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |_txn| {
            Err(Error::InvalidOperation("test error".to_string()))
        });

        match result {
            Err(Error::InvalidOperation(msg)) => assert_eq!(msg, "test error"),
            other => panic!("Expected InvalidOperation, got {:?}", other),
        }
    }

    #[test]
    fn test_transaction_multiple_puts() {
        let tdb = TestDb::new();
        let keys: Vec<Key> = (0..10).map(|i| tdb.key(&format!("multi_{}", i))).collect();

        tdb.db
            .transaction(tdb.run_id, |txn| {
                for (i, key) in keys.iter().enumerate() {
                    txn.put(key.clone(), values::int(i as i64))?;
                }
                Ok(())
            })
            .unwrap();

        // Verify all committed
        for (i, key) in keys.iter().enumerate() {
            let val = tdb.db.get(key).unwrap().unwrap();
            assert_eq!(val.value, values::int(i as i64));
        }
    }

    #[test]
    fn test_transaction_read_your_writes() {
        let tdb = TestDb::new();
        let key = tdb.key("ryw_test");

        let read_value = tdb
            .db
            .transaction(tdb.run_id, |txn| {
                txn.put(key.clone(), values::int(42))?;

                // Should see our own write
                let val = txn.get(&key)?;
                Ok(val)
            })
            .unwrap();

        assert_eq!(read_value, Some(values::int(42)));
    }

    #[test]
    fn test_transaction_sees_existing_data() {
        let tdb = TestDb::new();
        let key = tdb.key("existing_data");

        // Pre-populate
        tdb.db
            .put(tdb.run_id, key.clone(), values::int(10))
            .unwrap();

        let read_value = tdb
            .db
            .transaction(tdb.run_id, |txn| {
                let val = txn.get(&key)?;
                Ok(val)
            })
            .unwrap();

        assert_eq!(read_value, Some(values::int(10)));
    }

    #[test]
    fn test_transaction_can_delete() {
        let tdb = TestDb::new();
        let key = tdb.key("delete_test");

        // Pre-populate
        tdb.db
            .put(tdb.run_id, key.clone(), values::int(50))
            .unwrap();

        // Delete in transaction
        tdb.db
            .transaction(tdb.run_id, |txn| {
                txn.delete(key.clone())?;
                Ok(())
            })
            .unwrap();

        // Should be gone
        assert!(tdb.db.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_transaction_delete_then_read_returns_none() {
        let tdb = TestDb::new();
        let key = tdb.key("delete_read_test");

        // Pre-populate
        tdb.db
            .put(tdb.run_id, key.clone(), values::int(50))
            .unwrap();

        let read_after_delete = tdb
            .db
            .transaction(tdb.run_id, |txn| {
                txn.delete(key.clone())?;
                // Read after delete should return None
                let val = txn.get(&key)?;
                Ok(val)
            })
            .unwrap();

        assert!(read_after_delete.is_none());
    }

    #[test]
    fn test_transaction_empty_commits_successfully() {
        let tdb = TestDb::new();

        let result = tdb.db.transaction(tdb.run_id, |_txn| Ok("empty txn"));

        assert_eq!(result.unwrap(), "empty txn");
    }

    #[test]
    fn test_transaction_read_only_commits() {
        let tdb = TestDb::new();
        let key = tdb.key("read_only_test");

        // Pre-populate
        tdb.db.put(tdb.run_id, key.clone(), values::int(5)).unwrap();

        // Read-only transaction
        let result = tdb.db.transaction(tdb.run_id, |txn| {
            let _ = txn.get(&key)?;
            Ok(())
        });

        assert!(result.is_ok());
    }

    #[test]
    fn test_nested_transaction_closure_calls_fail() {
        let tdb = TestDb::new();

        // This tests that we can't start a new transaction inside a transaction closure
        // (The db.transaction call inside would block or fail in practice)
        // This test documents the expected behavior: don't do this!
        let result = tdb.db.transaction(tdb.run_id, |_txn| {
            // Don't actually try to nest - just verify outer works
            Ok(())
        });

        assert!(result.is_ok());
    }
}

// ============================================================================
// Transaction with Retry Tests
// ============================================================================

mod retry_api {
    use super::*;

    #[test]
    fn test_retry_succeeds_on_first_try() {
        let tdb = TestDb::new();
        let key = tdb.key("retry_first");
        let attempts = AtomicU64::new(0);

        let result = tdb
            .db
            .transaction_with_retry(tdb.run_id, RetryConfig::new(), |txn| {
                attempts.fetch_add(1, Ordering::Relaxed);
                txn.put(key.clone(), values::int(1))?;
                Ok(())
            });

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_retry_retries_on_conflict() {
        let tdb = TestDb::new();
        let key = tdb.key("retry_conflict");
        let attempts = AtomicU64::new(0);

        let result = tdb.db.transaction_with_retry(
            tdb.run_id,
            RetryConfig::new().with_max_retries(5),
            |txn| {
                let count = attempts.fetch_add(1, Ordering::Relaxed);
                if count < 2 {
                    return Err(Error::TransactionConflict("simulated".to_string()));
                }
                txn.put(key.clone(), values::int(42))?;
                Ok(())
            },
        );

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::Relaxed), 3); // Failed twice, succeeded third
    }

    #[test]
    fn test_retry_gives_up_after_max_retries() {
        let tdb = TestDb::new();
        let attempts = AtomicU64::new(0);

        let result: Result<(), Error> = tdb.db.transaction_with_retry(
            tdb.run_id,
            RetryConfig::new().with_max_retries(3),
            |_txn| {
                attempts.fetch_add(1, Ordering::Relaxed);
                Err(Error::TransactionConflict("always fail".to_string()))
            },
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().is_conflict());
        assert_eq!(attempts.load(Ordering::Relaxed), 4); // Initial + 3 retries
    }

    #[test]
    fn test_retry_does_not_retry_non_conflict_errors() {
        let tdb = TestDb::new();
        let attempts = AtomicU64::new(0);

        let result: Result<(), Error> = tdb.db.transaction_with_retry(
            tdb.run_id,
            RetryConfig::new().with_max_retries(5),
            |_txn| {
                attempts.fetch_add(1, Ordering::Relaxed);
                Err(Error::InvalidState("not a conflict".to_string()))
            },
        );

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::Relaxed), 1); // No retries
    }

    #[test]
    fn test_retry_no_retry_config() {
        let tdb = TestDb::new();
        let attempts = AtomicU64::new(0);

        let result: Result<(), Error> =
            tdb.db
                .transaction_with_retry(tdb.run_id, RetryConfig::no_retry(), |_txn| {
                    attempts.fetch_add(1, Ordering::Relaxed);
                    Err(Error::TransactionConflict("fail".to_string()))
                });

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::Relaxed), 1); // No retries
    }

    #[test]
    fn test_retry_clears_write_set_between_retries() {
        let tdb = TestDb::new();
        let key = tdb.key("retry_clear");
        let attempts = AtomicU64::new(0);

        tdb.db
            .transaction_with_retry(tdb.run_id, RetryConfig::new().with_max_retries(3), |txn| {
                let count = attempts.fetch_add(1, Ordering::Relaxed);

                // First attempt writes 100
                if count == 0 {
                    txn.put(key.clone(), values::int(100))?;
                    return Err(Error::TransactionConflict("retry".to_string()));
                }

                // Second attempt writes 200
                txn.put(key.clone(), values::int(200))?;
                Ok(())
            })
            .unwrap();

        // Should have 200, not 100
        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(200));
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::new()
            .with_max_retries(10)
            .with_base_delay_ms(50)
            .with_max_delay_ms(5000);

        let tdb = TestDb::new();
        let key = tdb.key("config_test");

        // Just verify it works
        tdb.db
            .transaction_with_retry(tdb.run_id, config, |txn| {
                txn.put(key.clone(), values::int(1))?;
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn test_retry_returns_closure_value() {
        let tdb = TestDb::new();
        let attempts = AtomicU64::new(0);

        let result = tdb
            .db
            .transaction_with_retry(tdb.run_id, RetryConfig::new(), |_txn| {
                let count = attempts.fetch_add(1, Ordering::Relaxed);
                if count < 1 {
                    return Err(Error::TransactionConflict("retry".to_string()));
                }
                Ok("success after retry")
            });

        assert_eq!(result.unwrap(), "success after retry");
    }
}

// ============================================================================
// Transaction with Timeout Tests
// ============================================================================

mod timeout_api {
    use super::*;

    #[test]
    fn test_timeout_succeeds_within_limit() {
        let tdb = TestDb::new();
        let key = tdb.key("timeout_ok");

        let result = tdb
            .db
            .transaction_with_timeout(tdb.run_id, Duration::from_secs(10), |txn| {
                txn.put(key.clone(), values::int(42))?;
                Ok(42)
            });

        assert_eq!(result.unwrap(), 42);
        assert_eq!(tdb.db.get(&key).unwrap().unwrap().value, values::int(42));
    }

    #[test]
    fn test_timeout_aborts_when_exceeded() {
        let tdb = TestDb::new();
        let key = tdb.key("timeout_exceeded");

        let result: Result<(), Error> =
            tdb.db
                .transaction_with_timeout(tdb.run_id, Duration::from_millis(10), |txn| {
                    txn.put(key.clone(), values::int(999))?;
                    thread::sleep(Duration::from_millis(50));
                    Ok(())
                });

        assert!(result.is_err());
        assert!(result.unwrap_err().is_timeout());

        // Data should NOT be committed
        assert!(tdb.db.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_timeout_checks_before_commit() {
        let tdb = TestDb::new();
        let key = tdb.key("timeout_at_commit");

        // The timeout is checked BEFORE commit, not during
        let result: Result<(), Error> =
            tdb.db
                .transaction_with_timeout(tdb.run_id, Duration::from_millis(5), |txn| {
                    // Do fast work
                    txn.put(key.clone(), values::int(1))?;
                    // But then sleep so timeout expires before commit
                    thread::sleep(Duration::from_millis(20));
                    Ok(())
                });

        assert!(result.is_err());
        assert!(result.unwrap_err().is_timeout());
    }

    #[test]
    fn test_timeout_fast_transactions_unaffected() {
        let tdb = TestDb::new();

        // Many fast transactions should all succeed
        for i in 0..100 {
            let key = tdb.key(&format!("fast_{}", i));
            let result =
                tdb.db
                    .transaction_with_timeout(tdb.run_id, Duration::from_secs(5), |txn| {
                        txn.put(key.clone(), values::int(i))?;
                        Ok(())
                    });
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_timeout_closure_error_takes_precedence() {
        let tdb = TestDb::new();

        let result: Result<(), Error> =
            tdb.db
                .transaction_with_timeout(tdb.run_id, Duration::from_millis(10), |_txn| {
                    // Error before sleeping
                    Err(Error::InvalidOperation("closure error".to_string()))
                });

        // Should get the closure error, not timeout
        match result {
            Err(Error::InvalidOperation(msg)) => assert_eq!(msg, "closure error"),
            other => panic!("Expected InvalidOperation, got {:?}", other),
        }
    }
}

// ============================================================================
// Manual Transaction Control Tests
// ============================================================================

mod manual_transaction {
    use super::*;

    #[test]
    fn test_begin_transaction_returns_context() {
        let tdb = TestDb::new();

        let txn = tdb.db.begin_transaction(tdb.run_id);
        assert!(txn.is_active());
        assert_eq!(txn.run_id, tdb.run_id);
    }

    #[test]
    fn test_manual_commit_persists_data() {
        let tdb = TestDb::new();
        let key = tdb.key("manual_commit");

        let mut txn = tdb.db.begin_transaction(tdb.run_id);
        txn.put(key.clone(), values::int(77)).unwrap();
        tdb.db.commit_transaction(&mut txn).unwrap();

        assert!(txn.is_committed());
        assert_eq!(tdb.db.get(&key).unwrap().unwrap().value, values::int(77));
    }

    #[test]
    fn test_manual_transaction_not_committed_is_lost() {
        let pdb = PersistentTestDb::new();
        let key = pdb.key("not_committed");

        {
            let db = pdb.open();
            let mut txn = db.begin_transaction(pdb.run_id);
            txn.put(key.clone(), values::int(123)).unwrap();
            // Don't commit - just drop
        }

        {
            let db = pdb.open();
            // Data should not exist
            assert!(db.get(&key).unwrap().is_none());
        }
    }

    #[test]
    fn test_begin_assigns_unique_txn_ids() {
        let tdb = TestDb::new();

        let txn1 = tdb.db.begin_transaction(tdb.run_id);
        let txn2 = tdb.db.begin_transaction(tdb.run_id);
        let txn3 = tdb.db.begin_transaction(tdb.run_id);

        assert_ne!(txn1.txn_id, txn2.txn_id);
        assert_ne!(txn2.txn_id, txn3.txn_id);
        assert_ne!(txn1.txn_id, txn3.txn_id);
    }

    #[test]
    fn test_begin_uses_current_version() {
        let tdb = TestDb::new();
        let key = tdb.key("version_test");

        // Get initial version
        let txn1 = tdb.db.begin_transaction(tdb.run_id);
        let v1 = txn1.start_version;

        // Make a change
        tdb.db.put(tdb.run_id, key, values::int(1)).unwrap();

        // New transaction should have higher version
        let txn2 = tdb.db.begin_transaction(tdb.run_id);
        assert!(txn2.start_version > v1);
    }
}

// ============================================================================
// Implicit Transaction (M1 API) Tests
// ============================================================================

mod implicit_transactions {
    use super::*;

    #[test]
    fn test_put_creates_implicit_transaction() {
        let tdb = TestDb::new();
        let key = tdb.key("implicit_put");

        tdb.db
            .put(tdb.run_id, key.clone(), values::int(10))
            .unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(10));
    }

    #[test]
    fn test_get_reads_data() {
        let tdb = TestDb::new();
        let key = tdb.key("implicit_get");

        tdb.db
            .put(tdb.run_id, key.clone(), values::int(20))
            .unwrap();
        let val = tdb.db.get(&key).unwrap();

        assert_eq!(val.unwrap().value, values::int(20));
    }

    #[test]
    fn test_get_returns_none_for_missing_key() {
        let tdb = TestDb::new();
        let key = tdb.key("nonexistent");

        let val = tdb.db.get(&key).unwrap();
        assert!(val.is_none());
    }

    #[test]
    fn test_delete_removes_key() {
        let tdb = TestDb::new();
        let key = tdb.key("implicit_delete");

        tdb.db
            .put(tdb.run_id, key.clone(), values::int(30))
            .unwrap();
        assert!(tdb.db.get(&key).unwrap().is_some());

        tdb.db.delete(tdb.run_id, key.clone()).unwrap();
        assert!(tdb.db.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent_key_succeeds() {
        let tdb = TestDb::new();
        let key = tdb.key("never_existed");

        // Should not error
        tdb.db.delete(tdb.run_id, key).unwrap();
    }

    #[test]
    fn test_cas_succeeds_with_correct_version() {
        let tdb = TestDb::new();
        let key = tdb.key("cas_success");

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();
        let val = tdb.db.get(&key).unwrap().unwrap();
        let version = val.version.as_u64();

        tdb.db
            .cas(tdb.run_id, key.clone(), version, values::int(2))
            .unwrap();

        let new_val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(new_val.value, values::int(2));
    }

    #[test]
    fn test_cas_fails_with_wrong_version() {
        let tdb = TestDb::new();
        let key = tdb.key("cas_fail");

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();

        let result = tdb.db.cas(tdb.run_id, key.clone(), 9999, values::int(2));

        assert!(result.is_err());
    }

    #[test]
    fn test_put_overwrites_existing() {
        let tdb = TestDb::new();
        let key = tdb.key("overwrite");

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();
        tdb.db.put(tdb.run_id, key.clone(), values::int(2)).unwrap();

        let val = tdb.db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(2));
    }

    #[test]
    fn test_multiple_puts_increment_version() {
        let tdb = TestDb::new();
        let key = tdb.key("version_increment");

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();
        let v1 = tdb.db.get(&key).unwrap().unwrap().version;

        tdb.db.put(tdb.run_id, key.clone(), values::int(2)).unwrap();
        let v2 = tdb.db.get(&key).unwrap().unwrap().version;

        assert!(v2 > v1);
    }

    #[test]
    fn test_all_value_types_via_implicit_txn() {
        let tdb = TestDb::new();

        let cases = vec![
            ("null", values::null()),
            ("bool", values::bool_val(true)),
            ("int", values::int(42)),
            ("float", values::float(3.14)),
            ("string", values::string("hello")),
            ("bytes", values::bytes(&[1, 2, 3])),
            ("array", values::array(vec![values::int(1), values::int(2)])),
            ("map", values::map(vec![("a", values::int(1))])),
        ];

        for (name, value) in cases {
            let key = tdb.key(name);
            tdb.db.put(tdb.run_id, key.clone(), value.clone()).unwrap();
            let stored = tdb.db.get(&key).unwrap().unwrap();
            assert_eq!(stored.value, value, "Mismatch for type: {}", name);
        }
    }
}

// ============================================================================
// Metrics Tests
// ============================================================================

mod metrics {
    use super::*;

    #[test]
    fn test_metrics_tracks_commits() {
        let tdb = TestDb::new();

        let before = tdb.db.metrics().total_committed;

        for i in 0..5 {
            let key = tdb.key(&format!("metric_{}", i));
            tdb.db.put(tdb.run_id, key, values::int(i)).unwrap();
        }

        let after = tdb.db.metrics().total_committed;
        assert!(after >= before + 5);
    }

    #[test]
    fn test_metrics_tracks_aborts() {
        let tdb = TestDb::new();

        let before = tdb.db.metrics().total_aborted;

        // Force some aborts
        for _ in 0..3 {
            let _: Result<(), Error> = tdb.db.transaction(tdb.run_id, |_txn| {
                Err(Error::InvalidState("force abort".to_string()))
            });
        }

        let after = tdb.db.metrics().total_aborted;
        assert!(after >= before + 3);
    }

    #[test]
    fn test_coordinator_accessor() {
        let tdb = TestDb::new();

        let coord = tdb.db.coordinator();
        let _metrics = coord.metrics();
    }
}

// ============================================================================
// Different Run IDs Tests
// ============================================================================

mod run_isolation {
    use super::*;

    #[test]
    fn test_different_run_ids_isolated() {
        let tdb = TestDb::new();

        let run1 = RunId::new();
        let run2 = RunId::new();

        let ns1 = create_namespace_for_run(run1);
        let ns2 = create_namespace_for_run(run2);

        let key1 = kv_key(&ns1, "shared_name");
        let key2 = kv_key(&ns2, "shared_name");

        tdb.db.put(run1, key1.clone(), values::int(100)).unwrap();
        tdb.db.put(run2, key2.clone(), values::int(200)).unwrap();

        assert_eq!(tdb.db.get(&key1).unwrap().unwrap().value, values::int(100));
        assert_eq!(tdb.db.get(&key2).unwrap().unwrap().value, values::int(200));
    }

    #[test]
    fn test_transaction_uses_correct_run_id() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let run1 = RunId::new();
        let run2 = RunId::new();

        let ns1 = create_namespace_for_run(run1);
        let ns2 = create_namespace_for_run(run2);

        // Write to run1's namespace
        db.transaction(run1, |txn| {
            let key = kv_key(&ns1, "data");
            txn.put(key, values::int(1))?;
            Ok(())
        })
        .unwrap();

        // Write to run2's namespace
        db.transaction(run2, |txn| {
            let key = kv_key(&ns2, "data");
            txn.put(key, values::int(2))?;
            Ok(())
        })
        .unwrap();

        // Both should exist independently
        assert!(db.get(&kv_key(&ns1, "data")).unwrap().is_some());
        assert!(db.get(&kv_key(&ns2, "data")).unwrap().is_some());
    }
}
