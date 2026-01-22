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
