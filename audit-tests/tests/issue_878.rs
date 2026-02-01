//! Audit test for issue #878: Storage errors silently treated as version 0 during transaction validation
//! Verdict: CONFIRMED BUG
//!
//! When store.get(key) returns Err(_) during validation, the code treats it as version 0.
//! For CAS operations with expected_version=0, this allows the CAS to succeed even though
//! the key's actual state is unknown (I/O error). This could lead to duplicate key creation.

use std::sync::Arc;
use std::time::Duration;

use strata_concurrency::TransactionContext;
use strata_core::traits::Storage;
use strata_core::types::{BranchId, Key, Namespace};
use strata_core::value::Value;
use strata_core::{StrataError, StrataResult, VersionedValue};
use strata_storage::ShardedStore;

fn create_namespace(branch_id: BranchId) -> Namespace {
    Namespace::new(
        "tenant".to_string(),
        "app".to_string(),
        "agent".to_string(),
        branch_id,
    )
}

fn create_key(ns: &Namespace, name: &str) -> Key {
    Key::new_kv(ns.clone(), name)
}

/// A mock storage that returns an error for specific keys.
/// This simulates an I/O failure during validation.
struct FailingStore {
    inner: Arc<ShardedStore>,
    fail_keys: Vec<Key>,
}

impl Storage for FailingStore {
    fn get(&self, key: &Key) -> StrataResult<Option<VersionedValue>> {
        if self.fail_keys.contains(key) {
            Err(StrataError::Storage {
                message: "Simulated I/O error".to_string(),
                source: None,
            })
        } else {
            self.inner.get(key)
        }
    }

    fn get_versioned(&self, key: &Key, max_version: u64) -> StrataResult<Option<VersionedValue>> {
        self.inner.get_versioned(key, max_version)
    }

    fn get_history(
        &self,
        key: &Key,
        limit: Option<usize>,
        before_version: Option<u64>,
    ) -> StrataResult<Vec<VersionedValue>> {
        self.inner.get_history(key, limit, before_version)
    }

    fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> StrataResult<u64> {
        Storage::put(self.inner.as_ref(), key, value, ttl)
    }

    fn delete(&self, key: &Key) -> StrataResult<Option<VersionedValue>> {
        Storage::delete(self.inner.as_ref(), key)
    }

    fn scan_prefix(
        &self,
        prefix: &Key,
        max_version: u64,
    ) -> StrataResult<Vec<(Key, VersionedValue)>> {
        self.inner.scan_prefix(prefix, max_version)
    }

    fn scan_by_branch(
        &self,
        branch_id: BranchId,
        max_version: u64,
    ) -> StrataResult<Vec<(Key, VersionedValue)>> {
        self.inner.scan_by_branch(branch_id, max_version)
    }

    fn current_version(&self) -> u64 {
        self.inner.current_version()
    }

    fn put_with_version(
        &self,
        key: Key,
        value: Value,
        version: u64,
        ttl: Option<Duration>,
    ) -> StrataResult<()> {
        self.inner.put_with_version(key, value, version, ttl)
    }

    fn delete_with_version(&self, key: &Key, version: u64) -> StrataResult<Option<VersionedValue>> {
        self.inner.delete_with_version(key, version)
    }
}

/// Demonstrates that a CAS with expected_version=0 succeeds when storage
/// returns an error, even though the key may actually exist.
///
/// This is the dangerous case: the CAS "create-if-not-exists" pattern
/// passes validation when it should fail or return an error.
#[test]
fn issue_878_cas_succeeds_on_storage_error() {
    let branch_id = BranchId::new();
    let ns = create_namespace(branch_id);
    let key = create_key(&ns, "important_key");

    let inner = Arc::new(ShardedStore::new());

    // Pre-populate the key at version 5 (it EXISTS)
    inner
        .put_with_version(key.clone(), Value::Int(42), 5, None)
        .unwrap();

    // Create a failing store that returns errors for this key
    let failing_store = FailingStore {
        inner: Arc::clone(&inner),
        fail_keys: vec![key.clone()],
    };

    // Create a transaction with CAS expected_version=0 (expects key to NOT exist)
    let snapshot = inner.snapshot();
    let mut txn = TransactionContext::with_snapshot(1, branch_id, Box::new(snapshot));
    txn.cas(key.clone(), 0, Value::Int(999)).unwrap();

    // Commit against the failing store
    // The CAS validation will get Err(_) from store.get(), treat it as version 0,
    // which matches expected_version=0, so the CAS PASSES.
    // This is the bug: the key actually exists at version 5!
    let result = txn.commit(&failing_store);

    // BUG: This succeeds when it should fail or return an error
    // The CAS expected the key to not exist (version 0), but we can't determine
    // the actual state because of the I/O error.
    assert!(
        result.is_ok(),
        "CAS incorrectly succeeds when storage returns I/O error (this demonstrates the bug)"
    );
}

/// Demonstrates that read-set validation with Err is actually conservative
/// (produces a false conflict when read_version != 0), but still swallows errors.
#[test]
fn issue_878_read_set_error_produces_false_conflict() {
    let branch_id = BranchId::new();
    let ns = create_namespace(branch_id);
    let key = create_key(&ns, "read_key");

    let inner = Arc::new(ShardedStore::new());

    // Pre-populate the key at version 3
    inner
        .put_with_version(key.clone(), Value::Int(100), 3, None)
        .unwrap();

    // Create a failing store
    let failing_store = FailingStore {
        inner: Arc::clone(&inner),
        fail_keys: vec![key.clone()],
    };

    // Create a transaction that reads the key (recording version 3 in read_set)
    let snapshot = inner.snapshot();
    let mut txn = TransactionContext::with_snapshot(1, branch_id, Box::new(snapshot));

    // Read the key from snapshot (records version 3 in read_set)
    let val = txn.get(&key).unwrap();
    assert!(val.is_some());

    // Write something so txn is not read-only
    let other_key = create_key(&ns, "other");
    txn.put(other_key, Value::Int(1)).unwrap();

    // Commit against failing store
    // For read_set validation: Err(_) -> version 0, read_version was 3, so 0 != 3 -> conflict
    // This IS conservative (false conflict), but the error is still silently swallowed
    let result = txn.commit(&failing_store);

    // This produces a false conflict -- the validation fails, but because of the wrong reason
    // (I/O error treated as version 0 instead of propagating the error)
    assert!(
        result.is_err(),
        "Read-set validation should produce a conflict (though for wrong reason)"
    );
}

/// Demonstrates the worst case: CAS with expected_version=0 on a key that exists,
/// where I/O error masks the existence check.
#[test]
fn issue_878_cas_create_if_not_exists_bypassed() {
    let branch_id = BranchId::new();
    let ns = create_namespace(branch_id);
    let unique_key = create_key(&ns, "unique_constraint_key");

    let inner = Arc::new(ShardedStore::new());

    // The key exists at version 10
    inner
        .put_with_version(
            unique_key.clone(),
            Value::String("existing".to_string()),
            10,
            None,
        )
        .unwrap();

    // Failing store can't read this key
    let failing_store = FailingStore {
        inner: Arc::clone(&inner),
        fail_keys: vec![unique_key.clone()],
    };

    // Transaction tries to create the key only if it doesn't exist (CAS v0)
    let snapshot = inner.snapshot();
    let mut txn = TransactionContext::with_snapshot(1, branch_id, Box::new(snapshot));
    txn.cas(
        unique_key.clone(),
        0, // Expected: key must not exist
        Value::String("new_value".to_string()),
    )
    .unwrap();

    // Bug: CAS passes validation because I/O error -> version 0 == expected 0
    let result = txn.commit(&failing_store);
    assert!(
        result.is_ok(),
        "BUG: CAS create-if-not-exists passes despite key existing (I/O error masks existence)"
    );
}
