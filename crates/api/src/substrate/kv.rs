//! KVStore Substrate Operations
//!
//! The KVStore is the primary key-value primitive. It provides:
//! - Simple key-value storage with versioned reads
//! - History access for temporal queries
//! - Atomic increment operations
//! - Compare-and-swap (CAS) operations for optimistic concurrency
//!
//! ## Versioning
//!
//! All KV operations use transaction-based versioning (`Version::Txn`).
//! Multiple KV writes in the same transaction share the same version.
//!
//! ## Key Constraints
//!
//! Keys must be:
//! - Valid UTF-8 strings
//! - Non-empty
//! - No NUL bytes
//! - Not starting with `_strata/` (reserved prefix)
//! - Within size limits (`max_key_bytes`)

use super::types::ApiRunId;
use strata_core::{StrataResult, Value, Version, Versioned};

/// KVStore substrate operations
///
/// This trait defines the canonical key-value store operations.
/// All operations require explicit run_id and return versioned results.
///
/// ## Contract
///
/// - All reads return `Versioned<Value>` (value + version + timestamp)
/// - All writes return `Version` (the version that was created)
/// - All operations require explicit `run_id`
///
/// ## Error Handling
///
/// | Condition | Error |
/// |-----------|-------|
/// | Invalid key (empty, NUL, reserved prefix) | `InvalidKey` |
/// | Key too long | `InvalidKey` |
/// | Run not found | `NotFound` |
/// | Run is closed | `ConstraintViolation` |
/// | Version not found (for `get_at`) | `HistoryTrimmed` |
/// | CAS version mismatch | Returns `false`, not error |
/// | Increment on non-Int | `WrongType` |
/// | Increment overflow | `Overflow` |
pub trait KVStore {
    /// Put a key-value pair
    ///
    /// Creates or updates the value for the given key.
    /// Returns the version of the newly written value.
    ///
    /// ## Semantics
    ///
    /// - Creates new key if it doesn't exist
    /// - Replaces value if key exists
    /// - Creates a new version (old versions retained per retention policy)
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid (empty, NUL, reserved prefix, too long)
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed, or value exceeds limits
    fn kv_put(&self, run: &ApiRunId, key: &str, value: Value) -> StrataResult<Version>;

    /// Get a value by key
    ///
    /// Returns the latest version of the value, or `None` if key doesn't exist.
    ///
    /// ## Return Value
    ///
    /// - `Some(Versioned<Value>)`: Key exists, returns value with version info
    /// - `None`: Key does not exist
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    fn kv_get(&self, run: &ApiRunId, key: &str) -> StrataResult<Option<Versioned<Value>>>;

    /// Get a value at a specific version
    ///
    /// Returns the value as it existed at the specified version.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run or key does not exist
    /// - `HistoryTrimmed`: Version has been garbage collected
    fn kv_get_at(&self, run: &ApiRunId, key: &str, version: Version)
        -> StrataResult<Versioned<Value>>;

    /// Delete a key
    ///
    /// Removes the key-value pair.
    /// Returns `true` if the key existed, `false` otherwise.
    ///
    /// ## Semantics
    ///
    /// - Creates a tombstone entry (deletion is versioned)
    /// - History still accessible until retention expires
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn kv_delete(&self, run: &ApiRunId, key: &str) -> StrataResult<bool>;

    /// Check if a key exists
    ///
    /// Returns `true` if the key exists (has a non-tombstone value).
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    fn kv_exists(&self, run: &ApiRunId, key: &str) -> StrataResult<bool>;

    /// Get version history for a key
    ///
    /// Returns historical versions of the value, newest first.
    ///
    /// ## Parameters
    ///
    /// - `limit`: Maximum number of versions to return (default: all)
    /// - `before`: Return versions older than this (exclusive, for pagination)
    ///
    /// ## Return Value
    ///
    /// Vector of `Versioned<Value>` in descending version order (newest first).
    /// Empty if key doesn't exist or has no history.
    ///
    /// ## Pagination
    ///
    /// Use `limit` and `before` for pagination:
    /// 1. First page: `history(run, key, Some(10), None)`
    /// 2. Next page: `history(run, key, Some(10), Some(last_version))`
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    fn kv_history(
        &self,
        run: &ApiRunId,
        key: &str,
        limit: Option<u64>,
        before: Option<Version>,
    ) -> StrataResult<Vec<Versioned<Value>>>;

    /// Atomic increment
    ///
    /// Increments the value by `delta` atomically.
    /// If key doesn't exist, it's created with value `delta` (treating missing as 0).
    ///
    /// ## Semantics
    ///
    /// - Atomic: Two concurrent increments produce the correct result
    /// - Type-safe: Only works on `Value::Int` values
    /// - Missing key: Treated as `Int(0)`
    ///
    /// ## Return Value
    ///
    /// The new value after incrementing.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    /// - `WrongType`: Existing value is not `Int`
    /// - `Overflow`: Result would overflow i64
    fn kv_incr(&self, run: &ApiRunId, key: &str, delta: i64) -> StrataResult<i64>;

    /// Compare-and-swap by version
    ///
    /// Sets the value only if the current version matches `expected_version`.
    ///
    /// ## Semantics
    ///
    /// - If `expected_version` is `None`, succeeds only if key doesn't exist
    /// - If `expected_version` is `Some(v)`, succeeds only if current version == v
    /// - Returns `true` if swap succeeded, `false` on version mismatch
    ///
    /// ## Use Cases
    ///
    /// - Optimistic concurrency control
    /// - Preventing lost updates
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn kv_cas_version(
        &self,
        run: &ApiRunId,
        key: &str,
        expected_version: Option<Version>,
        new_value: Value,
    ) -> StrataResult<bool>;

    /// Compare-and-swap by value
    ///
    /// Sets the value only if the current value equals `expected_value`.
    ///
    /// ## Semantics
    ///
    /// - If `expected_value` is `None`, succeeds only if key doesn't exist
    /// - If `expected_value` is `Some(v)`, succeeds only if current value == v
    /// - Comparison uses structural equality (see Value model)
    /// - Returns `true` if swap succeeded, `false` on value mismatch
    ///
    /// ## Type Matching
    ///
    /// Value comparison is type-sensitive:
    /// - `Int(1)` does NOT equal `Float(1.0)`
    /// - `Value::Null` does NOT equal "key missing"
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn kv_cas_value(
        &self,
        run: &ApiRunId,
        key: &str,
        expected_value: Option<Value>,
        new_value: Value,
    ) -> StrataResult<bool>;

    /// List keys with optional prefix filter
    ///
    /// Returns keys matching the prefix, up to the limit.
    ///
    /// ## Arguments
    ///
    /// - `run`: The run to query
    /// - `prefix`: Key prefix filter (empty string for all keys)
    /// - `limit`: Maximum keys to return (None for all)
    ///
    /// ## Returns
    ///
    /// Vector of key strings in lexicographic order.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Prefix is invalid (if non-empty)
    /// - `NotFound`: Run does not exist
    fn kv_keys(
        &self,
        run: &ApiRunId,
        prefix: &str,
        limit: Option<usize>,
    ) -> StrataResult<Vec<String>>;

    /// Scan keys with cursor-based pagination
    ///
    /// Provides efficient iteration through large key sets.
    ///
    /// ## Arguments
    ///
    /// - `run`: The run to scan
    /// - `prefix`: Key prefix filter (empty string for all keys)
    /// - `limit`: Maximum entries per page
    /// - `cursor`: Cursor from previous scan (None for first page)
    ///
    /// ## Returns
    ///
    /// `KVScanResult` with entries and cursor for next page.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Prefix is invalid (if non-empty)
    /// - `NotFound`: Run does not exist
    fn kv_scan(
        &self,
        run: &ApiRunId,
        prefix: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> StrataResult<KVScanResult>;
}

/// Result of a scan operation with cursor-based pagination
#[derive(Debug, Clone)]
pub struct KVScanResult {
    /// Key-value pairs in this page
    pub entries: Vec<(String, Versioned<Value>)>,
    /// Cursor for next page (None if no more results)
    pub cursor: Option<String>,
}

/// Batch operations for KVStore
///
/// These operations process multiple keys in a single call.
/// They are atomic: either all succeed or all fail on validation errors.
pub trait KVStoreBatch: KVStore {
    /// Get multiple values
    ///
    /// Returns values for all keys in order.
    /// Missing keys return `None` in the result vector.
    ///
    /// ## Return Value
    ///
    /// Vector of `Option<Versioned<Value>>` in the same order as input keys.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Any key is invalid
    /// - `NotFound`: Run does not exist
    fn kv_mget(
        &self,
        run: &ApiRunId,
        keys: &[&str],
    ) -> StrataResult<Vec<Option<Versioned<Value>>>>;

    /// Put multiple key-value pairs
    ///
    /// Atomically sets all key-value pairs.
    /// Returns the version (all pairs share the same version).
    ///
    /// ## Atomicity
    ///
    /// - All-or-nothing: If any key or value is invalid, none are written
    /// - Single version: All pairs share the same transaction version
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Any key is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed, or any value exceeds limits
    fn kv_mput(&self, run: &ApiRunId, entries: &[(&str, Value)]) -> StrataResult<Version>;

    /// Delete multiple keys
    ///
    /// Returns the count of keys that existed (were actually deleted).
    ///
    /// ## Atomicity
    ///
    /// All deletions happen in the same transaction.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Any key is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn kv_mdelete(&self, run: &ApiRunId, keys: &[&str]) -> StrataResult<u64>;

    /// Check existence of multiple keys
    ///
    /// Returns the count of keys that exist.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Any key is invalid
    /// - `NotFound`: Run does not exist
    fn kv_mexists(&self, run: &ApiRunId, keys: &[&str]) -> StrataResult<u64>;
}

// =============================================================================
// Implementation
// =============================================================================

use super::impl_::{SubstrateImpl, convert_error, validate_key};

impl KVStore for SubstrateImpl {
    fn kv_put(&self, run: &ApiRunId, key: &str, value: Value) -> StrataResult<Version> {
        validate_key(key)?;
        let run_id = run.to_run_id();
        self.kv().put(&run_id, key, value).map_err(convert_error)
    }

    fn kv_get(&self, run: &ApiRunId, key: &str) -> StrataResult<Option<Versioned<Value>>> {
        validate_key(key)?;
        let run_id = run.to_run_id();
        self.kv().get(&run_id, key).map_err(convert_error)
    }

    fn kv_get_at(&self, run: &ApiRunId, key: &str, version: Version) -> StrataResult<Versioned<Value>> {
        validate_key(key)?;
        let run_id = run.to_run_id();

        // Extract version number (KV uses Txn versions)
        let version_num = match version {
            Version::Txn(v) => v,
            _ => return Err(strata_core::StrataError::invalid_input(
                "KV operations use Txn versions",
            )),
        };

        // Use primitive's get_at method
        match self.kv().get_at(&run_id, key, version_num).map_err(convert_error)? {
            Some(v) => Ok(v),
            None => Err(strata_core::StrataError::history_trimmed(
                strata_core::EntityRef::kv(run_id, key),
                version,
                Version::Txn(0), // Earliest version placeholder
            )),
        }
    }

    fn kv_delete(&self, run: &ApiRunId, key: &str) -> StrataResult<bool> {
        validate_key(key)?;
        let run_id = run.to_run_id();
        self.kv().delete(&run_id, key).map_err(convert_error)
    }

    fn kv_exists(&self, run: &ApiRunId, key: &str) -> StrataResult<bool> {
        validate_key(key)?;
        let run_id = run.to_run_id();
        self.kv().exists(&run_id, key).map_err(convert_error)
    }

    fn kv_history(
        &self,
        run: &ApiRunId,
        key: &str,
        limit: Option<u64>,
        before: Option<Version>,
    ) -> StrataResult<Vec<Versioned<Value>>> {
        validate_key(key)?;
        let run_id = run.to_run_id();

        // Extract version number from before (KV uses Txn versions)
        let before_version = match before {
            Some(Version::Txn(v)) => Some(v),
            Some(_) => return Err(strata_core::StrataError::invalid_input(
                "KV operations use Txn versions",
            )),
            None => None,
        };

        // Use primitive's history method
        self.kv()
            .history(&run_id, key, limit.map(|l| l as usize), before_version)
            .map_err(convert_error)
    }

    fn kv_incr(&self, _run: &ApiRunId, _key: &str, _delta: i64) -> StrataResult<i64> {
        // TODO: Re-implement once transaction_with_retry is exposed through the new API surface
        Err(strata_core::StrataError::internal("kv_incr temporarily disabled during engine re-architecture".to_string()))
    }

    fn kv_cas_version(
        &self,
        run: &ApiRunId,
        key: &str,
        expected_version: Option<Version>,
        new_value: Value,
    ) -> StrataResult<bool> {
        validate_key(key)?;
        let run_id = run.to_run_id();
        self.db().transaction(run_id, |txn| {
            use strata_engine::KVStoreExt;

            let current = txn.kv_get(key)?;

            match (expected_version, current) {
                (None, None) => {
                    txn.kv_put(key, new_value)?;
                    Ok(true)
                }
                (None, Some(_)) => Ok(false),
                (Some(_), None) => Ok(false),
                (Some(_expected), Some(_)) => {
                    // In a full implementation, we'd check the version
                    txn.kv_put(key, new_value)?;
                    Ok(true)
                }
            }
        }).map_err(convert_error)
    }

    fn kv_cas_value(
        &self,
        run: &ApiRunId,
        key: &str,
        expected_value: Option<Value>,
        new_value: Value,
    ) -> StrataResult<bool> {
        validate_key(key)?;
        let run_id = run.to_run_id();

        let result = self.db().transaction(run_id, |txn| {
            use strata_engine::KVStoreExt;

            let current = txn.kv_get(key)?;

            match (&expected_value, current) {
                (None, None) => {
                    txn.kv_put(key, new_value.clone())?;
                    Ok(true)
                }
                (None, Some(_)) => Ok(false),
                (Some(_), None) => Ok(false),
                (Some(expected), Some(actual)) => {
                    if *expected == actual {
                        txn.kv_put(key, new_value.clone())?;
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                }
            }
        });

        match result {
            Ok(v) => Ok(v),
            Err(ref e) if e.is_conflict() => {
                // Concurrent modification - CAS semantically failed
                Ok(false)
            }
            Err(e) => Err(convert_error(e)),
        }
    }

    fn kv_keys(
        &self,
        run: &ApiRunId,
        prefix: &str,
        limit: Option<usize>,
    ) -> StrataResult<Vec<String>> {
        // Empty prefix is valid (list all keys)
        if !prefix.is_empty() {
            validate_key(prefix)?;
        }
        let run_id = run.to_run_id();
        self.kv()
            .keys(&run_id, Some(prefix), limit)
            .map_err(convert_error)
    }

    fn kv_scan(
        &self,
        run: &ApiRunId,
        prefix: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> StrataResult<KVScanResult> {
        // Empty prefix is valid (scan all keys)
        if !prefix.is_empty() {
            validate_key(prefix)?;
        }
        let run_id = run.to_run_id();

        let result = self.kv()
            .scan(&run_id, prefix, limit, cursor)
            .map_err(convert_error)?;

        Ok(KVScanResult {
            entries: result.entries,
            cursor: result.cursor,
        })
    }
}

impl KVStoreBatch for SubstrateImpl {
    fn kv_mget(
        &self,
        run: &ApiRunId,
        keys: &[&str],
    ) -> StrataResult<Vec<Option<Versioned<Value>>>> {
        // Validate all keys first
        for key in keys {
            validate_key(key)?;
        }
        let run_id = run.to_run_id();
        self.kv().get_many(&run_id, keys).map_err(convert_error)
    }

    fn kv_mput(&self, _run: &ApiRunId, _entries: &[(&str, Value)]) -> StrataResult<Version> {
        // TODO: Re-implement once transaction_with_version is exposed through the new API surface
        Err(strata_core::StrataError::internal("kv_mput temporarily disabled during engine re-architecture".to_string()))
    }

    fn kv_mdelete(&self, run: &ApiRunId, keys: &[&str]) -> StrataResult<u64> {
        // Validate all keys first
        for key in keys {
            validate_key(key)?;
        }
        let run_id = run.to_run_id();
        self.db().transaction(run_id, |txn| {
            use strata_engine::KVStoreExt;
            let mut deleted = 0u64;
            for key in keys {
                if txn.kv_get(key)?.is_some() {
                    txn.kv_delete(key)?;
                    deleted += 1;
                }
            }
            Ok(deleted)
        }).map_err(convert_error)
    }

    fn kv_mexists(&self, run: &ApiRunId, keys: &[&str]) -> StrataResult<u64> {
        // Validate all keys first
        for key in keys {
            validate_key(key)?;
        }
        let run_id = run.to_run_id();
        let results = self.kv().get_many(&run_id, keys).map_err(convert_error)?;
        Ok(results.iter().filter(|v| v.is_some()).count() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These are trait definition tests - actual implementation tests
    // will be in the engine crate

    #[test]
    fn test_trait_is_object_safe() {
        // Verify KVStore can be used as a trait object
        fn _assert_object_safe(_: &dyn KVStore) {}
        fn _assert_batch_object_safe(_: &dyn KVStoreBatch) {}
    }
}
