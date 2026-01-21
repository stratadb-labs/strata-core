//! TransactionContext API Unit Tests
//!
//! Comprehensive tests for TransactionContext methods and state transitions.

use super::test_utils::*;
use strata_concurrency::transaction::{TransactionContext, TransactionStatus};
use strata_core::error::Error;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// ============================================================================
// Construction Tests
// ============================================================================

mod construction {
    use super::*;

    #[test]
    fn test_new_creates_active_transaction() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        assert!(txn.is_active());
        assert_eq!(txn.txn_id, 1);
        assert_eq!(txn.run_id, run_id);
        assert_eq!(txn.start_version, 100);
    }

    #[test]
    fn test_new_initializes_empty_sets() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        assert!(txn.read_set.is_empty());
        assert!(txn.write_set.is_empty());
        assert!(txn.delete_set.is_empty());
        assert!(txn.cas_set.is_empty());
    }

    #[test]
    fn test_with_snapshot_creates_from_database() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "existing");

        // Pre-populate
        db.put(run_id, key.clone(), values::int(42)).unwrap();

        // Create transaction with snapshot
        let txn = db.begin_transaction(run_id);

        // Should be able to read existing data
        assert!(txn.is_active());
    }

    #[test]
    fn test_transaction_has_start_time() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        // Should have a valid elapsed time (very small)
        let elapsed = txn.elapsed();
        assert!(elapsed < Duration::from_secs(1));
    }
}

// ============================================================================
// Read Operation Tests
// ============================================================================

mod read_operations {
    use super::*;

    #[test]
    fn test_get_returns_none_for_missing_key() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "nonexistent");

        let mut txn = db.begin_transaction(run_id);
        let result = txn.get(&key).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_get_returns_existing_value() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "existing");

        db.put(run_id, key.clone(), values::int(99)).unwrap();

        let mut txn = db.begin_transaction(run_id);
        let result = txn.get(&key).unwrap();

        assert_eq!(result, Some(values::int(99)));
    }

    #[test]
    fn test_get_adds_to_read_set() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "tracked");

        db.put(run_id, key.clone(), values::int(1)).unwrap();

        let mut txn = db.begin_transaction(run_id);
        assert!(txn.read_set.is_empty());

        let _ = txn.get(&key);
        assert!(txn.read_set.contains_key(&key));
    }

    #[test]
    fn test_get_read_version() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "versioned");

        db.put(run_id, key.clone(), values::int(1)).unwrap();

        let mut txn = db.begin_transaction(run_id);
        let _ = txn.get(&key);

        let version = txn.get_read_version(&key);
        assert!(version.is_some());
    }

    #[test]
    fn test_exists_returns_false_for_missing() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "missing");

        let mut txn = db.begin_transaction(run_id);
        assert!(!txn.exists(&key).unwrap());
    }

    #[test]
    fn test_exists_returns_true_for_existing() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "exists");

        db.put(run_id, key.clone(), values::int(1)).unwrap();

        let mut txn = db.begin_transaction(run_id);
        assert!(txn.exists(&key).unwrap());
    }

    #[test]
    fn test_scan_prefix_returns_matching_keys() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        // Create keys with common prefix
        for i in 0..5 {
            let key = kv_key(&ns, &format!("scan_test_{}", i));
            db.put(run_id, key, values::int(i)).unwrap();
        }

        // Create key with different prefix
        let other_key = kv_key(&ns, "other_key");
        db.put(run_id, other_key, values::int(999)).unwrap();

        let mut txn = db.begin_transaction(run_id);
        let prefix = kv_key(&ns, "scan_test_");
        let results = txn.scan_prefix(&prefix).unwrap();

        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_scan_prefix_empty_result() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        let mut txn = db.begin_transaction(run_id);
        let prefix = kv_key(&ns, "nonexistent_prefix_");
        let results = txn.scan_prefix(&prefix).unwrap();

        assert!(results.is_empty());
    }
}

// ============================================================================
// Write Operation Tests
// ============================================================================

mod write_operations {
    use super::*;

    #[test]
    fn test_put_adds_to_write_set() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "write_test");

        let mut txn = db.begin_transaction(run_id);
        assert!(txn.write_set.is_empty());

        txn.put(key.clone(), values::int(42)).unwrap();
        assert!(txn.write_set.contains_key(&key));
        assert_eq!(txn.write_set.get(&key), Some(&values::int(42)));
    }

    #[test]
    fn test_put_overwrites_previous_put() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "overwrite");

        let mut txn = db.begin_transaction(run_id);
        txn.put(key.clone(), values::int(1)).unwrap();
        txn.put(key.clone(), values::int(2)).unwrap();

        assert_eq!(txn.write_set.get(&key), Some(&values::int(2)));
    }

    #[test]
    fn test_delete_adds_to_delete_set() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "delete_test");

        let mut txn = db.begin_transaction(run_id);
        txn.delete(key.clone()).unwrap();

        assert!(txn.delete_set.contains(&key));
    }

    #[test]
    fn test_delete_removes_from_write_set() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "delete_write");

        let mut txn = db.begin_transaction(run_id);
        txn.put(key.clone(), values::int(42)).unwrap();
        assert!(txn.write_set.contains_key(&key));

        txn.delete(key.clone()).unwrap();
        assert!(!txn.write_set.contains_key(&key));
        assert!(txn.delete_set.contains(&key));
    }

    #[test]
    fn test_cas_adds_to_cas_set() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "cas_test");

        db.put(run_id, key.clone(), values::int(1)).unwrap();
        let version = db.get(&key).unwrap().unwrap().version.as_u64();

        let mut txn = db.begin_transaction(run_id);
        txn.cas(key.clone(), version, values::int(2)).unwrap();

        assert_eq!(txn.cas_set.len(), 1);
    }

    #[test]
    fn test_multiple_cas_operations() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        // Create multiple keys
        let keys: Vec<_> = (0..3)
            .map(|i| {
                let key = kv_key(&ns, &format!("cas_{}", i));
                db.put(run_id, key.clone(), values::int(i)).unwrap();
                key
            })
            .collect();

        let mut txn = db.begin_transaction(run_id);

        for (i, key) in keys.iter().enumerate() {
            let version = db.get(key).unwrap().unwrap().version.as_u64();
            txn.cas(key.clone(), version, values::int((i + 10) as i64))
                .unwrap();
        }

        assert_eq!(txn.cas_set.len(), 3);
    }

    #[test]
    fn test_clear_operations_resets_all_sets() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key1 = kv_key(&ns, "key1");
        let key2 = kv_key(&ns, "key2");
        let key3 = kv_key(&ns, "key3");

        db.put(run_id, key3.clone(), values::int(1)).unwrap();
        let version = db.get(&key3).unwrap().unwrap().version.as_u64();

        let mut txn = db.begin_transaction(run_id);
        txn.put(key1.clone(), values::int(1)).unwrap();
        txn.delete(key2.clone()).unwrap();
        txn.cas(key3.clone(), version, values::int(2)).unwrap();

        assert!(!txn.write_set.is_empty());
        assert!(!txn.delete_set.is_empty());
        assert!(!txn.cas_set.is_empty());

        txn.clear_operations().unwrap();

        assert!(txn.write_set.is_empty());
        assert!(txn.delete_set.is_empty());
        assert!(txn.cas_set.is_empty());
    }
}

// ============================================================================
// Read-Your-Writes Tests
// ============================================================================

mod read_your_writes {
    use super::*;

    #[test]
    fn test_get_sees_own_put() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "ryw");

        let mut txn = db.begin_transaction(run_id);
        txn.put(key.clone(), values::int(42)).unwrap();

        let read = txn.get(&key).unwrap();
        assert_eq!(read, Some(values::int(42)));
    }

    #[test]
    fn test_get_sees_own_overwrite() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "ryw_overwrite");

        db.put(run_id, key.clone(), values::int(1)).unwrap();

        let mut txn = db.begin_transaction(run_id);

        // Read original
        assert_eq!(txn.get(&key).unwrap(), Some(values::int(1)));

        // Overwrite
        txn.put(key.clone(), values::int(2)).unwrap();

        // Should see new value
        assert_eq!(txn.get(&key).unwrap(), Some(values::int(2)));
    }

    #[test]
    fn test_get_sees_delete_as_none() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "ryw_delete");

        db.put(run_id, key.clone(), values::int(1)).unwrap();

        let mut txn = db.begin_transaction(run_id);
        assert_eq!(txn.get(&key).unwrap(), Some(values::int(1)));

        txn.delete(key.clone()).unwrap();
        assert_eq!(txn.get(&key).unwrap(), None);
    }

    #[test]
    fn test_exists_sees_own_put() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "ryw_exists");

        let mut txn = db.begin_transaction(run_id);
        assert!(!txn.exists(&key).unwrap());

        txn.put(key.clone(), values::int(1)).unwrap();
        assert!(txn.exists(&key).unwrap());
    }

    #[test]
    fn test_exists_sees_own_delete() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "ryw_exists_del");

        db.put(run_id, key.clone(), values::int(1)).unwrap();

        let mut txn = db.begin_transaction(run_id);
        assert!(txn.exists(&key).unwrap());

        txn.delete(key.clone()).unwrap();
        assert!(!txn.exists(&key).unwrap());
    }

    #[test]
    fn test_scan_prefix_includes_pending_writes() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        // Pre-populate
        for i in 0..3 {
            let key = kv_key(&ns, &format!("prefix_{}", i));
            db.put(run_id, key, values::int(i)).unwrap();
        }

        let mut txn = db.begin_transaction(run_id);

        // Add more in transaction
        for i in 3..5 {
            let key = kv_key(&ns, &format!("prefix_{}", i));
            txn.put(key, values::int(i)).unwrap();
        }

        let prefix = kv_key(&ns, "prefix_");
        let results = txn.scan_prefix(&prefix).unwrap();

        // Should see all 5
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_scan_prefix_excludes_pending_deletes() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        // Pre-populate
        for i in 0..5 {
            let key = kv_key(&ns, &format!("scan_del_{}", i));
            db.put(run_id, key, values::int(i)).unwrap();
        }

        let mut txn = db.begin_transaction(run_id);

        // Delete some
        txn.delete(kv_key(&ns, "scan_del_1")).unwrap();
        txn.delete(kv_key(&ns, "scan_del_3")).unwrap();

        let prefix = kv_key(&ns, "scan_del_");
        let results = txn.scan_prefix(&prefix).unwrap();

        // Should see 3 (0, 2, 4)
        assert_eq!(results.len(), 3);
    }
}

// ============================================================================
// State Transition Tests
// ============================================================================

mod state_transitions {
    use super::*;

    #[test]
    fn test_initial_state_is_active() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        assert!(txn.is_active());
        assert!(!txn.is_committed());
        assert!(!txn.is_aborted());
    }

    #[test]
    fn test_mark_validating_transitions_from_active() {
        let run_id = RunId::new();
        let mut txn = TransactionContext::new(1, run_id, 100);

        txn.mark_validating().unwrap();

        assert!(!txn.is_active());
        match txn.status {
            TransactionStatus::Validating => {}
            _ => panic!("Expected Validating status"),
        }
    }

    #[test]
    fn test_mark_committed_transitions_from_validating() {
        let run_id = RunId::new();
        let mut txn = TransactionContext::new(1, run_id, 100);

        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();

        assert!(txn.is_committed());
    }

    #[test]
    fn test_mark_aborted_from_active() {
        let run_id = RunId::new();
        let mut txn = TransactionContext::new(1, run_id, 100);

        txn.mark_aborted("test reason".to_string()).unwrap();

        assert!(txn.is_aborted());
    }

    #[test]
    fn test_mark_aborted_from_validating() {
        let run_id = RunId::new();
        let mut txn = TransactionContext::new(1, run_id, 100);

        txn.mark_validating().unwrap();
        txn.mark_aborted("validation failed".to_string()).unwrap();

        assert!(txn.is_aborted());
    }

    #[test]
    fn test_cannot_transition_from_committed() {
        let run_id = RunId::new();
        let mut txn = TransactionContext::new(1, run_id, 100);

        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();

        // Cannot abort a committed transaction
        let result = txn.mark_aborted("too late".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_transition_from_aborted() {
        let run_id = RunId::new();
        let mut txn = TransactionContext::new(1, run_id, 100);

        txn.mark_aborted("reason".to_string()).unwrap();

        // Cannot commit an aborted transaction
        let result = txn.mark_validating();
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_active_succeeds_when_active() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        txn.ensure_active().unwrap();
    }

    #[test]
    fn test_ensure_active_fails_when_not_active() {
        let run_id = RunId::new();
        let mut txn = TransactionContext::new(1, run_id, 100);

        txn.mark_aborted("test".to_string()).unwrap();

        let result = txn.ensure_active();
        assert!(result.is_err());
    }

    #[test]
    fn test_can_rollback_when_active() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        assert!(txn.can_rollback());
    }

    #[test]
    fn test_cannot_rollback_when_committed() {
        let run_id = RunId::new();
        let mut txn = TransactionContext::new(1, run_id, 100);

        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();

        assert!(!txn.can_rollback());
    }
}

// ============================================================================
// Timeout Tests
// ============================================================================

mod timeout {
    use super::*;

    #[test]
    fn test_is_expired_false_immediately() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        assert!(!txn.is_expired(Duration::from_secs(1)));
    }

    #[test]
    fn test_is_expired_true_after_sleep() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        thread::sleep(Duration::from_millis(50));

        assert!(txn.is_expired(Duration::from_millis(10)));
        assert!(!txn.is_expired(Duration::from_secs(1)));
    }

    #[test]
    fn test_elapsed_increases_over_time() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        let t1 = txn.elapsed();
        thread::sleep(Duration::from_millis(50));
        let t2 = txn.elapsed();

        assert!(t2 > t1);
        assert!(t2 >= Duration::from_millis(50));
    }

    #[test]
    fn test_zero_timeout_always_expired() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        // With zero timeout, should be expired immediately
        // (or very close to it - allow tiny margin)
        thread::sleep(Duration::from_millis(1));
        assert!(txn.is_expired(Duration::ZERO));
    }
}

// ============================================================================
// Pending Operations Tests
// ============================================================================

mod pending_operations {
    use super::*;

    #[test]
    fn test_pending_operations_empty_initially() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(1, run_id, 100);

        let pending = txn.pending_operations();
        assert_eq!(pending.puts, 0);
        assert_eq!(pending.deletes, 0);
        assert_eq!(pending.cas, 0);
    }

    #[test]
    fn test_pending_operations_counts_writes() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        let mut txn = db.begin_transaction(run_id);

        for i in 0..5 {
            let key = kv_key(&ns, &format!("key_{}", i));
            txn.put(key, values::int(i)).unwrap();
        }

        let pending = txn.pending_operations();
        assert_eq!(pending.puts, 5);
    }

    #[test]
    fn test_pending_operations_counts_deletes() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        let mut txn = db.begin_transaction(run_id);

        for i in 0..3 {
            let key = kv_key(&ns, &format!("del_{}", i));
            txn.delete(key).unwrap();
        }

        let pending = txn.pending_operations();
        assert_eq!(pending.deletes, 3);
    }

    #[test]
    fn test_pending_operations_counts_cas() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        // Pre-populate
        for i in 0..2 {
            let key = kv_key(&ns, &format!("cas_{}", i));
            db.put(run_id, key, values::int(i)).unwrap();
        }

        let mut txn = db.begin_transaction(run_id);

        for i in 0..2 {
            let key = kv_key(&ns, &format!("cas_{}", i));
            let version = db.get(&key).unwrap().unwrap().version.as_u64();
            txn.cas(key, version, values::int(i + 10)).unwrap();
        }

        let pending = txn.pending_operations();
        assert_eq!(pending.cas, 2);
    }
}

// ============================================================================
// Operations on Non-Active Transaction Tests
// ============================================================================

mod non_active_operations {
    use super::*;

    #[test]
    fn test_put_fails_on_aborted_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "test");

        let mut txn = db.begin_transaction(run_id);
        txn.mark_aborted("test".to_string()).unwrap();

        let result = txn.put(key, values::int(1));
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_fails_on_aborted_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "test");

        let mut txn = db.begin_transaction(run_id);
        txn.mark_aborted("test".to_string()).unwrap();

        let result = txn.delete(key);
        assert!(result.is_err());
    }

    #[test]
    fn test_cas_fails_on_committed_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "test");

        db.put(run_id, key.clone(), values::int(1)).unwrap();

        let mut txn = db.begin_transaction(run_id);
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();

        let result = txn.cas(key, 1, values::int(2));
        assert!(result.is_err());
    }
}

// ============================================================================
// Value Type Tests
// ============================================================================

mod value_types {
    use super::*;

    #[test]
    fn test_put_all_value_types() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();

        let cases = vec![
            ("null", values::null()),
            ("bool_true", values::bool_val(true)),
            ("bool_false", values::bool_val(false)),
            ("int_pos", values::int(i64::MAX)),
            ("int_neg", values::int(i64::MIN)),
            ("float", values::float(std::f64::consts::PI)),
            ("string", values::string("hello world")),
            ("string_empty", values::string("")),
            ("bytes", values::bytes(&[0, 1, 255])),
            (
                "array",
                values::array(vec![values::int(1), values::string("a")]),
            ),
            (
                "map",
                values::map(vec![("key", values::int(42)), ("nested", values::null())]),
            ),
        ];

        for (name, value) in cases {
            let key = kv_key(&ns, name);

            db.transaction(run_id, |txn| {
                txn.put(key.clone(), value.clone())?;
                Ok(())
            })
            .unwrap();

            let stored = db.get(&key).unwrap().unwrap();
            assert_eq!(stored.value, value, "Mismatch for: {}", name);
        }
    }

    #[test]
    fn test_large_value() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path().join("db")).unwrap();

        let (run_id, ns) = create_namespace();
        let key = kv_key(&ns, "large");

        // 1MB value
        let large = values::large_bytes(1024);

        db.transaction(run_id, |txn| {
            txn.put(key.clone(), large.clone())?;
            Ok(())
        })
        .unwrap();

        let stored = db.get(&key).unwrap().unwrap();
        assert_eq!(stored.value, large);
    }
}
