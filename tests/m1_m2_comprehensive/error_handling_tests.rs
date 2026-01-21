//! Error Handling Tests
//!
//! Tests for error propagation, error types, and error recovery.

use super::test_utils::*;
use strata_core::error::Error;
use strata_core::value::Value;
use strata_engine::{Database, RetryConfig};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Error Type Tests
// ============================================================================

mod error_types {
    use super::*;

    #[test]
    fn test_transaction_conflict_error() {
        let tdb = TestDb::new();
        let key = tdb.key("conflict_error");

        tdb.db.put(tdb.run_id, key.clone(), values::int(0)).unwrap();

        // Simulate conflict
        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |_txn| {
            Err(Error::TransactionConflict("simulated conflict".to_string()))
        });

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_conflict());
        assert!(!err.is_timeout());
    }

    #[test]
    fn test_transaction_timeout_error() {
        let tdb = TestDb::new();
        let key = tdb.key("timeout_error");

        let result: Result<(), Error> =
            tdb.db
                .transaction_with_timeout(tdb.run_id, Duration::from_millis(5), |txn| {
                    txn.put(key.clone(), values::int(1))?;
                    thread::sleep(Duration::from_millis(20));
                    Ok(())
                });

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.is_timeout());
        assert!(!err.is_conflict());
    }

    #[test]
    fn test_invalid_state_error() {
        let tdb = TestDb::new();

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |_txn| {
            Err(Error::InvalidState("test invalid state".to_string()))
        });

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::InvalidState(msg) => assert_eq!(msg, "test invalid state"),
            e => panic!("Expected InvalidState, got {:?}", e),
        }
    }

    #[test]
    fn test_invalid_operation_error() {
        let tdb = TestDb::new();

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |_txn| {
            Err(Error::InvalidOperation("test invalid op".to_string()))
        });

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::InvalidOperation(msg) => assert_eq!(msg, "test invalid op"),
            e => panic!("Expected InvalidOperation, got {:?}", e),
        }
    }

    #[test]
    fn test_version_mismatch_error() {
        let tdb = TestDb::new();
        let key = tdb.key("version_mismatch");

        tdb.db.put(tdb.run_id, key.clone(), values::int(1)).unwrap();
        let actual_version = tdb.db.get(&key).unwrap().unwrap().version.as_u64();

        // CAS with wrong version
        let result = tdb.db.cas(
            tdb.run_id,
            key.clone(),
            actual_version + 100,
            values::int(2),
        );

        assert!(result.is_err());
        // The error type depends on implementation
    }

    #[test]
    fn test_storage_error_propagates() {
        let tdb = TestDb::new();

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |_txn| {
            Err(Error::StorageError("storage issue".to_string()))
        });

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::StorageError(msg) => assert_eq!(msg, "storage issue"),
            e => panic!("Expected StorageError, got {:?}", e),
        }
    }

    #[test]
    fn test_corruption_error() {
        let tdb = TestDb::new();

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |_txn| {
            Err(Error::Corruption("data corruption detected".to_string()))
        });

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Corruption(msg) => assert_eq!(msg, "data corruption detected"),
            e => panic!("Expected Corruption, got {:?}", e),
        }
    }
}

// ============================================================================
// Error Propagation Tests
// ============================================================================

mod error_propagation {
    use super::*;

    #[test]
    fn test_error_from_closure_aborts_transaction() {
        let tdb = TestDb::new();
        let key = tdb.key("abort_on_error");

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(key.clone(), values::int(42))?;

            // Error after put - should abort
            Err(Error::InvalidState("closure error".to_string()))
        });

        assert!(result.is_err());

        // Key should not exist (aborted)
        assert!(tdb.db.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_error_type_preserved_through_transaction() {
        let tdb = TestDb::new();

        // Various error types should be preserved
        let errors = vec![
            Error::InvalidState("state error".to_string()),
            Error::InvalidOperation("op error".to_string()),
            Error::StorageError("storage error".to_string()),
            Error::Corruption("corruption error".to_string()),
            Error::TransactionConflict("conflict error".to_string()),
        ];

        for original_error in errors {
            let error_clone = original_error.to_string();

            let result: Result<(), Error> =
                tdb.db.transaction(tdb.run_id, |_txn| Err(original_error));

            assert!(result.is_err());
            let returned_error = result.unwrap_err();
            assert_eq!(returned_error.to_string(), error_clone);
        }
    }

    #[test]
    fn test_nested_function_errors_propagate() {
        let tdb = TestDb::new();
        let key = tdb.key("nested_error");

        fn inner_function() -> Result<(), Error> {
            Err(Error::InvalidOperation("inner error".to_string()))
        }

        fn middle_function() -> Result<(), Error> {
            inner_function()?;
            Ok(())
        }

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(key.clone(), values::int(1))?;
            middle_function()?;
            Ok(())
        });

        assert!(result.is_err());
        assert!(tdb.db.get(&key).unwrap().is_none()); // Aborted
    }

    #[test]
    fn test_question_mark_operator_works() {
        let tdb = TestDb::new();
        let key1 = tdb.key("q1");
        let key2 = tdb.key("q2");

        let result: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(key1.clone(), values::int(1))?; // Should work
            txn.put(key2.clone(), values::int(2))?; // Should work
            Err(Error::InvalidState("final error".to_string())) // Fails
        });

        assert!(result.is_err());

        // Neither should exist
        assert!(tdb.db.get(&key1).unwrap().is_none());
        assert!(tdb.db.get(&key2).unwrap().is_none());
    }
}

// ============================================================================
// Retry Error Handling Tests
// ============================================================================

mod retry_error_handling {
    use super::*;

    #[test]
    fn test_only_conflict_errors_retry() {
        let tdb = TestDb::new();
        let attempts = AtomicU64::new(0);

        // InvalidState should NOT trigger retry
        let result: Result<(), Error> = tdb.db.transaction_with_retry(
            tdb.run_id,
            RetryConfig::new().with_max_retries(10),
            |_txn| {
                attempts.fetch_add(1, Ordering::Relaxed);
                Err(Error::InvalidState("not a conflict".to_string()))
            },
        );

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::Relaxed), 1); // No retries
    }

    #[test]
    fn test_conflict_errors_do_retry() {
        let tdb = TestDb::new();
        let attempts = AtomicU64::new(0);

        let result: Result<(), Error> = tdb.db.transaction_with_retry(
            tdb.run_id,
            RetryConfig::new().with_max_retries(3),
            |_txn| {
                attempts.fetch_add(1, Ordering::Relaxed);
                Err(Error::TransactionConflict("simulated".to_string()))
            },
        );

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::Relaxed), 4); // 1 initial + 3 retries
    }

    #[test]
    fn test_timeout_errors_do_not_retry() {
        let tdb = TestDb::new();
        let attempts = AtomicU64::new(0);

        let result: Result<(), Error> = tdb.db.transaction_with_retry(
            tdb.run_id,
            RetryConfig::new().with_max_retries(10),
            |_txn| {
                attempts.fetch_add(1, Ordering::Relaxed);
                Err(Error::TransactionTimeout("timeout".to_string()))
            },
        );

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::Relaxed), 1); // No retries
    }

    #[test]
    fn test_success_after_retries_returns_value() {
        let tdb = TestDb::new();
        let attempts = AtomicU64::new(0);

        let result = tdb.db.transaction_with_retry(
            tdb.run_id,
            RetryConfig::new().with_max_retries(10),
            |_txn| {
                let count = attempts.fetch_add(1, Ordering::Relaxed);
                if count < 3 {
                    Err(Error::TransactionConflict("retry".to_string()))
                } else {
                    Ok("success")
                }
            },
        );

        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn test_max_retries_exceeded_returns_conflict() {
        let tdb = TestDb::new();

        let result: Result<(), Error> = tdb.db.transaction_with_retry(
            tdb.run_id,
            RetryConfig::new().with_max_retries(2),
            |_txn| Err(Error::TransactionConflict("always fail".to_string())),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().is_conflict());
    }

    #[test]
    fn test_non_conflict_error_after_retries() {
        let tdb = TestDb::new();
        let attempts = AtomicU64::new(0);

        // First few fail with conflict, then fail with different error
        let result: Result<(), Error> = tdb.db.transaction_with_retry(
            tdb.run_id,
            RetryConfig::new().with_max_retries(10),
            |_txn| {
                let count = attempts.fetch_add(1, Ordering::Relaxed);
                if count < 2 {
                    Err(Error::TransactionConflict("retry".to_string()))
                } else {
                    Err(Error::InvalidState("final error".to_string()))
                }
            },
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::InvalidState(_) => {} // Expected
            e => panic!("Expected InvalidState, got {:?}", e),
        }
        assert_eq!(attempts.load(Ordering::Relaxed), 3);
    }
}

// ============================================================================
// Error Recovery Tests
// ============================================================================

mod error_recovery {
    use super::*;

    #[test]
    fn test_database_usable_after_error() {
        let tdb = TestDb::new();
        let key = tdb.key("after_error");

        // Force error
        let _: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
            txn.put(key.clone(), values::int(1))?;
            Err(Error::InvalidState("error".to_string()))
        });

        // Should still work
        tdb.db
            .transaction(tdb.run_id, |txn| {
                txn.put(key.clone(), values::int(42))?;
                Ok(())
            })
            .unwrap();

        assert_eq!(tdb.db.get(&key).unwrap().unwrap().value, values::int(42));
    }

    #[test]
    fn test_database_usable_after_many_errors() {
        let tdb = TestDb::new();

        // Many errors
        for i in 0..100 {
            let key = tdb.key(&format!("error_{}", i));
            let _: Result<(), Error> = tdb.db.transaction(tdb.run_id, |txn| {
                txn.put(key.clone(), values::int(i))?;
                Err(Error::InvalidState("error".to_string()))
            });
        }

        // Should still work
        let key = tdb.key("after_errors");
        tdb.db
            .put(tdb.run_id, key.clone(), values::int(999))
            .unwrap();
        assert!(tdb.db.get(&key).unwrap().is_some());
    }

    #[test]
    fn test_database_usable_after_timeout() {
        let tdb = TestDb::new();
        let key1 = tdb.key("timeout_key");
        let key2 = tdb.key("after_timeout");

        // Timeout
        let _: Result<(), Error> =
            tdb.db
                .transaction_with_timeout(tdb.run_id, Duration::from_millis(5), |txn| {
                    txn.put(key1.clone(), values::int(1))?;
                    thread::sleep(Duration::from_millis(20));
                    Ok(())
                });

        // Should still work
        tdb.db
            .put(tdb.run_id, key2.clone(), values::int(42))
            .unwrap();
        assert!(tdb.db.get(&key2).unwrap().is_some());
    }

    #[test]
    fn test_concurrent_errors_dont_affect_others() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        let success_count = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let db = Arc::clone(&db);
                let ns = ns.clone();
                let success_count = Arc::clone(&success_count);

                thread::spawn(move || {
                    let key = kv_key(&ns, &format!("concurrent_{}", i));

                    // Half succeed, half fail
                    let result: Result<(), Error> = if i % 2 == 0 {
                        db.transaction(run_id, |txn| {
                            txn.put(key.clone(), values::int(i))?;
                            Ok(())
                        })
                    } else {
                        db.transaction(run_id, |txn| {
                            txn.put(key.clone(), values::int(i))?;
                            Err(Error::InvalidState("fail".to_string()))
                        })
                    };

                    if result.is_ok() {
                        success_count.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // Exactly 5 should have succeeded (0, 2, 4, 6, 8)
        assert_eq!(success_count.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_new_transactions_after_conflicts() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "conflict_key");

        db.put(run_id, key.clone(), values::int(0)).unwrap();

        // Generate conflicts
        let barrier = Arc::new(std::sync::Barrier::new(5));

        let handles: Vec<_> = (0..5)
            .map(|i| {
                let db = Arc::clone(&db);
                let key = key.clone();
                let barrier = Arc::clone(&barrier);

                thread::spawn(move || {
                    barrier.wait();
                    let _: Result<(), Error> = db.transaction(run_id, |txn| {
                        let _ = txn.get(&key)?;
                        thread::sleep(Duration::from_millis(10));
                        txn.put(key.clone(), values::int(i))?;
                        Ok(())
                    });
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // New transaction should work fine
        let new_key = kv_key(&ns, "new_key");
        db.put(run_id, new_key.clone(), values::int(100)).unwrap();
        assert!(db.get(&new_key).unwrap().is_some());
    }
}

// ============================================================================
// Error Message Tests
// ============================================================================

mod error_messages {
    use super::*;

    #[test]
    fn test_timeout_error_includes_duration() {
        let tdb = TestDb::new();
        let key = tdb.key("timeout_msg");

        let result: Result<(), Error> =
            tdb.db
                .transaction_with_timeout(tdb.run_id, Duration::from_millis(5), |txn| {
                    txn.put(key.clone(), values::int(1))?;
                    thread::sleep(Duration::from_millis(20));
                    Ok(())
                });

        if let Err(Error::TransactionTimeout(msg)) = result {
            // Message should mention the timeout
            assert!(msg.contains("5"), "Message should mention timeout: {}", msg);
        } else {
            panic!("Expected TransactionTimeout error");
        }
    }

    #[test]
    fn test_error_display_is_meaningful() {
        let errors = vec![
            Error::InvalidState("test state".to_string()),
            Error::InvalidOperation("test op".to_string()),
            Error::StorageError("test storage".to_string()),
            Error::Corruption("test corruption".to_string()),
            Error::TransactionConflict("test conflict".to_string()),
            Error::TransactionTimeout("test timeout".to_string()),
        ];

        for error in errors {
            let display = format!("{}", error);
            assert!(!display.is_empty(), "Error display should not be empty");
            // Should contain some of the error message
        }
    }
}

// ============================================================================
// Panic Safety Tests
// ============================================================================

mod panic_safety {
    use super::*;
    use std::panic;

    #[test]
    fn test_panic_in_transaction_doesnt_corrupt_database() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "panic_key");

        // Set up initial state
        db.put(run_id, key.clone(), values::int(1)).unwrap();

        // This would panic inside transaction (but we catch it)
        let db_clone = Arc::clone(&db);
        let key_clone = key.clone();
        let result = panic::catch_unwind(panic::AssertUnwindSafe(move || {
            let _: Result<(), Error> = db_clone.transaction(run_id, |txn| {
                txn.put(key_clone.clone(), values::int(999))?;
                panic!("Intentional panic");
            });
        }));

        assert!(result.is_err()); // Panic was caught

        // Database should still be usable and have original value
        let val = db.get(&key).unwrap().unwrap();
        assert_eq!(val.value, values::int(1)); // Original value preserved
    }

    #[test]
    fn test_database_usable_after_caught_panic() {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path().join("db")).unwrap());

        let (run_id, ns) = create_namespace();

        // Cause panic in another thread
        let db_clone = Arc::clone(&db);
        let ns_clone = ns.clone();
        let handle = thread::spawn(move || {
            let key = kv_key(&ns_clone, "panic_thread");
            let _: Result<(), Error> = db_clone.transaction(run_id, |txn| {
                txn.put(key, values::int(1))?;
                panic!("Thread panic");
            });
        });

        // Thread will panic
        assert!(handle.join().is_err());

        // Database should still be usable from main thread
        let key = kv_key(&ns, "after_panic");
        db.put(run_id, key.clone(), values::int(42)).unwrap();
        assert!(db.get(&key).unwrap().is_some());
    }
}
