# KV Perfection Plan

> **Status**: Ready for Implementation
> **Created**: 2026-01-23
> **Scope**: Make KVStore pass all 161 tests with 0 failures

---

## Executive Summary

This plan addresses the 7 failing KV tests and 15 ignored tests by implementing missing functionality across three layers:
1. **Storage Layer** (generic) - Add history iteration capability
2. **Primitive Layer** (KV-specific) - Add `get_at`, `history`, `scan` methods
3. **Substrate Layer** (API) - Wire methods + fix concurrency issues

---

## Current State

| Metric | Count |
|--------|-------|
| Passing | 139 |
| Failing | 7 |
| Ignored | 15 |
| **Total** | **161** |

### Failing Tests

| Test | Root Cause |
|------|------------|
| `kv::recovery_invariants::test_version_history_get_at` | `kv_get_at` stubbed |
| `kv::recovery_invariants::test_version_history_enumeration` | `kv_history` returns `vec![]` |
| `kv::recovery_invariants::test_version_history_survives_crash` | `kv_get_at` stubbed |
| `kv::concurrency::test_incr_atomic_under_concurrency` | No retry on WriteConflict |
| `kv::concurrency::test_cas_value_under_concurrency` | Error instead of `false` |
| `kv::transactions::test_incr_atomic_isolation` | No retry on WriteConflict |
| `kv::transactions::test_cas_retry_pattern` | Error instead of `false` |

### Ignored Tests (14 for kv_keys/kv_scan)

| Test Group | Count | Reason |
|------------|-------|--------|
| `kv_scan_ops::test_kv_keys_*` | 6 | `kv_keys` not implemented |
| `kv_scan_ops::test_kv_scan_*` | 8 | `kv_scan` not implemented |
| `kv_atomic_ops::test_incr_overflow_returns_error` | 1 | Already fixed, can unignore |

---

## Phase A: Storage Layer (Generic)

### A1: Add `VersionChain::history()` Method

**File**: `crates/storage/src/sharded.rs`

**Location**: After `get_at_version()` method (around line 93)

```rust
/// Get version history (newest first)
///
/// Returns versions in descending order (newest first).
/// Optionally limited and filtered by `before_version`.
///
/// # Arguments
/// * `limit` - Maximum versions to return (None = all)
/// * `before_version` - Only return versions older than this (exclusive, for pagination)
///
/// # Returns
/// Vector of StoredValue references, newest first
pub fn history(&self, limit: Option<usize>, before_version: Option<u64>) -> Vec<&StoredValue> {
    let iter = self.versions.iter();

    // Filter by before_version if specified
    let filtered: Box<dyn Iterator<Item = &StoredValue>> = match before_version {
        Some(before) => Box::new(iter.filter(move |sv| sv.version().as_u64() < before)),
        None => Box::new(iter),
    };

    // Apply limit if specified
    match limit {
        Some(n) => filtered.take(n).collect(),
        None => filtered.collect(),
    }
}

/// Get total number of versions in the chain
pub fn len(&self) -> usize {
    self.versions.len()
}
```

**Acceptance Criteria**:
- [ ] Returns versions in newest-first order
- [ ] `limit` parameter caps result count
- [ ] `before_version` filters to older versions only
- [ ] Works with empty chain (returns empty vec)
- [ ] No KV-specific code (generic for all primitives)

---

### A2: Add `Storage::get_history()` to Trait

**File**: `crates/core/src/traits.rs`

**Location**: After `get_versioned()` method (around line 46)

```rust
/// Get version history for a key
///
/// Returns historical versions of the value, newest first.
/// Used by all primitives that support history queries.
///
/// # Arguments
/// * `key` - The key to get history for
/// * `limit` - Maximum versions to return (None = all)
/// * `before_version` - Only return versions older than this (for pagination)
///
/// # Returns
/// Vector of VersionedValue in descending version order (newest first).
/// Empty if key doesn't exist or has no history.
///
/// # Errors
/// Returns an error if the storage operation fails.
fn get_history(
    &self,
    key: &Key,
    limit: Option<u64>,
    before_version: Option<u64>,
) -> Result<Vec<VersionedValue>>;
```

---

### A3: Implement `get_history()` in ShardedStore

**File**: `crates/storage/src/sharded.rs`

**Location**: In `impl Storage for ShardedStore` block

```rust
fn get_history(
    &self,
    key: &Key,
    limit: Option<u64>,
    before_version: Option<u64>,
) -> Result<Vec<VersionedValue>> {
    let run_id = key.namespace.run_id;

    Ok(self.shards.get(&run_id)
        .and_then(|shard| shard.data.get(key))
        .map(|chain| {
            chain.history(limit.map(|n| n as usize), before_version)
                .into_iter()
                .filter(|sv| !sv.is_expired())
                .map(|sv| sv.versioned().clone())
                .collect()
        })
        .unwrap_or_default())
}
```

---

### A4: Add `SnapshotView::get_history()` (Optional)

**File**: `crates/core/src/traits.rs`

If history queries need snapshot isolation:

```rust
/// Get version history from snapshot
///
/// Returns versions as they existed at snapshot time.
fn get_history(
    &self,
    key: &Key,
    limit: Option<u64>,
    before_version: Option<u64>,
) -> Result<Vec<VersionedValue>>;
```

**Note**: May not be needed if history queries go through Storage directly.

---

## Phase B: Primitive Layer (KV-Specific)

### B1: Add `KVStore::get_at()` Method

**File**: `crates/primitives/src/kv.rs`

**Location**: After `get()` method (around line 126)

```rust
/// Get a value at a specific version (point-in-time read)
///
/// Returns the value as it existed at or before the specified version.
/// This enables historical queries and replay scenarios.
///
/// # Arguments
/// * `run_id` - The run to query
/// * `key` - The key to look up
/// * `version` - The maximum version to return (inclusive)
///
/// # Returns
/// * `Ok(Some(Versioned<Value>))` - Value at that version
/// * `Ok(None)` - Key didn't exist at that version
///
/// # Errors
/// Returns error if storage operation fails.
pub fn get_at(
    &self,
    run_id: &RunId,
    key: &str,
    version: u64,
) -> Result<Option<Versioned<Value>>> {
    use strata_core::traits::Storage;

    let storage_key = self.key_for(run_id, key);
    self.db.storage().get_versioned(&storage_key, version)
}
```

---

### B2: Add `KVStore::history()` Method

**File**: `crates/primitives/src/kv.rs`

**Location**: After `get_at()` method

```rust
/// Get version history for a key
///
/// Returns historical versions of the value, newest first.
///
/// # Arguments
/// * `run_id` - The run to query
/// * `key` - The key to get history for
/// * `limit` - Maximum versions to return (None = all)
/// * `before_version` - Only return versions older than this (for pagination)
///
/// # Returns
/// Vector of Versioned<Value> in descending version order.
/// Empty if key doesn't exist or has no history.
///
/// # Example
///
/// ```ignore
/// // Get last 10 versions
/// let history = kv.history(&run_id, "key", Some(10), None)?;
///
/// // Paginate: get next 10 after version 50
/// let page2 = kv.history(&run_id, "key", Some(10), Some(50))?;
/// ```
pub fn history(
    &self,
    run_id: &RunId,
    key: &str,
    limit: Option<u64>,
    before_version: Option<u64>,
) -> Result<Vec<Versioned<Value>>> {
    use strata_core::traits::Storage;

    let storage_key = self.key_for(run_id, key);
    self.db.storage().get_history(&storage_key, limit, before_version)
}
```

---

### B3: Add `KVStore::scan()` Method with Cursor

**File**: `crates/primitives/src/kv.rs`

**Location**: After `list_with_values()` method

```rust
/// Scan result with cursor for pagination
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// Key-value pairs in this page
    pub entries: Vec<(String, Versioned<Value>)>,
    /// Cursor for next page (None if no more results)
    pub cursor: Option<String>,
}

/// Scan keys with cursor-based pagination
///
/// Provides efficient pagination through large key sets.
///
/// # Arguments
/// * `run_id` - The run to scan
/// * `prefix` - Key prefix filter (empty string for all keys)
/// * `limit` - Maximum entries per page
/// * `cursor` - Cursor from previous scan (None for first page)
///
/// # Returns
/// ScanResult with entries and cursor for next page.
///
/// # Example
///
/// ```ignore
/// // First page
/// let page1 = kv.scan(&run_id, "user:", 100, None)?;
///
/// // Next page
/// if let Some(cursor) = page1.cursor {
///     let page2 = kv.scan(&run_id, "user:", 100, Some(&cursor))?;
/// }
/// ```
pub fn scan(
    &self,
    run_id: &RunId,
    prefix: &str,
    limit: usize,
    cursor: Option<&str>,
) -> Result<ScanResult> {
    let all_entries = self.list_with_values(run_id, Some(prefix))?;

    // Find starting position based on cursor
    let start_idx = match cursor {
        Some(c) => all_entries
            .iter()
            .position(|(k, _)| k.as_str() > c)
            .unwrap_or(all_entries.len()),
        None => 0,
    };

    // Take limit + 1 to detect if there are more
    let entries: Vec<_> = all_entries
        .into_iter()
        .skip(start_idx)
        .take(limit + 1)
        .collect();

    // Check if there are more results
    let (entries, cursor) = if entries.len() > limit {
        let mut entries = entries;
        entries.pop(); // Remove the extra one
        let last_key = entries.last().map(|(k, _)| k.clone());
        (entries, last_key)
    } else {
        (entries, None)
    };

    Ok(ScanResult { entries, cursor })
}

/// List keys with optional prefix and limit
///
/// Simpler alternative to scan() when pagination isn't needed.
pub fn keys(
    &self,
    run_id: &RunId,
    prefix: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<String>> {
    let all_keys = self.list(run_id, prefix)?;

    match limit {
        Some(n) => Ok(all_keys.into_iter().take(n).collect()),
        None => Ok(all_keys),
    }
}
```

---

## Phase C: Substrate Layer (API)

### C1: Fix `kv_get_at` Implementation

**File**: `crates/api/src/substrate/kv.rs`

**Replace** lines 323-337:

```rust
fn kv_get_at(&self, run: &ApiRunId, key: &str, version: Version) -> StrataResult<Versioned<Value>> {
    validate_key(key)?;
    let run_id = run.to_run_id();

    // Extract version number
    let version_num = match version {
        Version::Txn(v) => v,
        _ => return Err(strata_core::StrataError::invalid_operation(
            "KV operations use Txn versions".to_string(),
        )),
    };

    // Use primitive's get_at method
    match self.kv().get_at(&run_id, key, version_num).map_err(convert_error)? {
        Some(v) => Ok(v),
        None => Err(strata_core::StrataError::history_trimmed(
            strata_core::EntityRef::kv(run_id, key),
            version,
            Version::Txn(0), // TODO: Get actual earliest version
        )),
    }
}
```

---

### C2: Fix `kv_history` Implementation

**File**: `crates/api/src/substrate/kv.rs`

**Replace** lines 352-362:

```rust
fn kv_history(
    &self,
    run: &ApiRunId,
    key: &str,
    limit: Option<u64>,
    before: Option<Version>,
) -> StrataResult<Vec<Versioned<Value>>> {
    validate_key(key)?;
    let run_id = run.to_run_id();

    // Extract version number from before
    let before_version = match before {
        Some(Version::Txn(v)) => Some(v),
        Some(_) => return Err(strata_core::StrataError::invalid_operation(
            "KV operations use Txn versions".to_string(),
        )),
        None => None,
    };

    // Use primitive's history method
    self.kv()
        .history(&run_id, key, limit, before_version)
        .map_err(convert_error)
}
```

---

### C3: Fix `kv_incr` with Retry Loop

**File**: `crates/api/src/substrate/kv.rs`

**Replace** lines 364-387:

```rust
fn kv_incr(&self, run: &ApiRunId, key: &str, delta: i64) -> StrataResult<i64> {
    validate_key(key)?;
    let run_id = run.to_run_id();

    const MAX_RETRIES: usize = 10;

    for attempt in 0..MAX_RETRIES {
        let result = self.db().transaction(run_id, |txn| {
            use strata_primitives::extensions::KVStoreExt;

            let current = txn.kv_get(key)?;
            let current_value = match current {
                Some(Value::Int(n)) => n,
                Some(_) => return Err(strata_core::error::Error::InvalidOperation(
                    "Cannot increment non-integer value".to_string(),
                )),
                None => 0,
            };

            // Use checked_add to prevent overflow
            let new_value = current_value.checked_add(delta).ok_or_else(|| {
                strata_core::error::Error::InvalidOperation(
                    "Integer overflow in increment operation".to_string(),
                )
            })?;

            txn.kv_put(key, Value::Int(new_value))?;
            Ok(new_value)
        });

        match result {
            Ok(v) => return Ok(v),
            Err(e) if is_write_conflict(&e) && attempt < MAX_RETRIES - 1 => {
                // Retry on write conflict
                std::thread::yield_now();
                continue;
            }
            Err(e) => return Err(convert_error(e)),
        }
    }

    Err(strata_core::StrataError::conflict(
        strata_core::EntityRef::kv(run_id, key),
        "Max retries exceeded for atomic increment".to_string(),
    ))
}

/// Check if an error is a write conflict (retriable)
fn is_write_conflict(e: &strata_core::error::Error) -> bool {
    matches!(e, strata_core::error::Error::WriteConflict { .. })
}
```

---

### C4: Fix `kv_cas_value` to Return `false` on Conflict

**File**: `crates/api/src/substrate/kv.rs`

**Replace** lines 420-451:

```rust
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
        use strata_primitives::extensions::KVStoreExt;

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
        Err(e) if is_write_conflict(&e) => {
            // Concurrent modification - CAS semantically failed
            Ok(false)
        }
        Err(e) => Err(convert_error(e)),
    }
}
```

---

### C5: Add `kv_keys` Trait Method and Implementation

**File**: `crates/api/src/substrate/kv.rs`

**Add to `KVStore` trait** (after `kv_cas_value`):

```rust
/// List keys with optional prefix filter
///
/// Returns keys matching the prefix, up to the limit.
///
/// # Arguments
/// * `run` - The run to query
/// * `prefix` - Key prefix filter (empty string for all)
/// * `limit` - Maximum keys to return
///
/// # Returns
/// Vector of key strings in lexicographic order.
fn kv_keys(
    &self,
    run: &ApiRunId,
    prefix: &str,
    limit: Option<usize>,
) -> StrataResult<Vec<String>>;
```

**Add implementation**:

```rust
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
```

---

### C6: Add `kv_scan` Trait Method and Implementation

**File**: `crates/api/src/substrate/kv.rs`

**Add types** (at top of file):

```rust
/// Result of a scan operation with cursor-based pagination
#[derive(Debug, Clone)]
pub struct KVScanResult {
    /// Key-value pairs in this page
    pub entries: Vec<(String, Versioned<Value>)>,
    /// Cursor for next page (None if no more results)
    pub cursor: Option<String>,
}
```

**Add to `KVStore` trait**:

```rust
/// Scan keys with cursor-based pagination
///
/// Provides efficient iteration through large key sets.
///
/// # Arguments
/// * `run` - The run to scan
/// * `prefix` - Key prefix filter
/// * `limit` - Maximum entries per page
/// * `cursor` - Cursor from previous scan (None for first page)
///
/// # Returns
/// KVScanResult with entries and cursor for next page.
fn kv_scan(
    &self,
    run: &ApiRunId,
    prefix: &str,
    limit: usize,
    cursor: Option<&str>,
) -> StrataResult<KVScanResult>;
```

**Add implementation**:

```rust
fn kv_scan(
    &self,
    run: &ApiRunId,
    prefix: &str,
    limit: usize,
    cursor: Option<&str>,
) -> StrataResult<KVScanResult> {
    // Empty prefix is valid
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
```

---

## Phase D: Test Updates

### D1: Unignore Fixed Tests

**File**: `tests/substrate_api_comprehensive/kv/atomic_ops.rs`

Remove `#[ignore]` from:
```rust
#[test]
fn test_incr_overflow_returns_error() {
    // This test should now pass with checked_add
}
```

### D2: Unignore kv_keys Tests

**File**: `tests/substrate_api_comprehensive/kv/scan_ops.rs`

Remove `#[ignore]` from all `test_kv_keys_*` tests.

### D3: Unignore kv_scan Tests

**File**: `tests/substrate_api_comprehensive/kv/scan_ops.rs`

Remove `#[ignore]` from all `test_kv_scan_*` tests.

### D4: Add Storage Layer Tests

**File**: `crates/storage/src/sharded.rs` (in `#[cfg(test)]` module)

```rust
#[test]
fn test_version_chain_history() {
    let mut chain = VersionChain::new(StoredValue::new(
        Value::Int(1),
        Version::Txn(1),
        Timestamp::now(),
    ));
    chain.push(StoredValue::new(Value::Int(2), Version::Txn(2), Timestamp::now()));
    chain.push(StoredValue::new(Value::Int(3), Version::Txn(3), Timestamp::now()));

    // All versions, newest first
    let all = chain.history(None, None);
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].version().as_u64(), 3);
    assert_eq!(all[2].version().as_u64(), 1);

    // Limited
    let limited = chain.history(Some(2), None);
    assert_eq!(limited.len(), 2);

    // With before filter
    let before = chain.history(None, Some(3));
    assert_eq!(before.len(), 2); // versions 1 and 2
}

#[test]
fn test_version_chain_history_empty() {
    let chain = VersionChain::new(StoredValue::new(
        Value::Int(1),
        Version::Txn(1),
        Timestamp::now(),
    ));

    // Before first version returns empty
    let before = chain.history(None, Some(1));
    assert!(before.is_empty());
}
```

### D5: Add Primitive Layer Tests

**File**: `crates/primitives/src/kv.rs` (in `#[cfg(test)]` module)

```rust
#[test]
fn test_get_at_returns_historical_version() {
    let (_temp, _db, kv) = setup();
    let run_id = RunId::new();

    let v1 = kv.put(&run_id, "key", Value::Int(1)).unwrap();
    let v2 = kv.put(&run_id, "key", Value::Int(2)).unwrap();
    let v3 = kv.put(&run_id, "key", Value::Int(3)).unwrap();

    // Get at each version
    let at_v1 = kv.get_at(&run_id, "key", v1.as_u64()).unwrap();
    let at_v2 = kv.get_at(&run_id, "key", v2.as_u64()).unwrap();
    let at_v3 = kv.get_at(&run_id, "key", v3.as_u64()).unwrap();

    assert_eq!(at_v1.map(|v| v.value), Some(Value::Int(1)));
    assert_eq!(at_v2.map(|v| v.value), Some(Value::Int(2)));
    assert_eq!(at_v3.map(|v| v.value), Some(Value::Int(3)));
}

#[test]
fn test_history_returns_all_versions() {
    let (_temp, _db, kv) = setup();
    let run_id = RunId::new();

    kv.put(&run_id, "key", Value::Int(1)).unwrap();
    kv.put(&run_id, "key", Value::Int(2)).unwrap();
    kv.put(&run_id, "key", Value::Int(3)).unwrap();

    let history = kv.history(&run_id, "key", None, None).unwrap();

    assert_eq!(history.len(), 3);
    // Newest first
    assert_eq!(history[0].value, Value::Int(3));
    assert_eq!(history[1].value, Value::Int(2));
    assert_eq!(history[2].value, Value::Int(1));
}

#[test]
fn test_scan_pagination() {
    let (_temp, _db, kv) = setup();
    let run_id = RunId::new();

    // Create 10 keys
    for i in 0..10 {
        kv.put(&run_id, &format!("key:{:02}", i), Value::Int(i)).unwrap();
    }

    // Scan with limit 3
    let page1 = kv.scan(&run_id, "key:", 3, None).unwrap();
    assert_eq!(page1.entries.len(), 3);
    assert!(page1.cursor.is_some());

    // Next page
    let page2 = kv.scan(&run_id, "key:", 3, page1.cursor.as_deref()).unwrap();
    assert_eq!(page2.entries.len(), 3);

    // Continue until exhausted
    let mut cursor = page2.cursor;
    let mut total = 6;
    while cursor.is_some() {
        let page = kv.scan(&run_id, "key:", 3, cursor.as_deref()).unwrap();
        total += page.entries.len();
        cursor = page.cursor;
    }
    assert_eq!(total, 10);
}
```

---

## Verification

### After Each Phase

**Phase A (Storage)**:
```bash
cargo test -p strata-storage version_chain_history
cargo test -p strata-core traits
```

**Phase B (Primitive)**:
```bash
cargo test -p strata-primitives kv::tests
```

**Phase C (Substrate)**:
```bash
cargo test --test substrate_api_comprehensive kv::
```

### Final Verification

```bash
# All KV tests should pass
cargo test --test substrate_api_comprehensive kv::

# Expected result:
# test result: ok. 161 passed; 0 failed; 1 ignored; 0 measured
# (1 ignored = overflow test if we want to keep it ignored)
```

---

## Success Criteria

| Criterion | Target |
|-----------|--------|
| Passing tests | 160+ |
| Failing tests | 0 |
| Ignored tests | ≤1 (overflow only) |
| `kv_get_at` works | Historical reads return correct version |
| `kv_history` works | Returns all versions, newest first |
| `kv_incr` atomic | No failures under 10-thread contention |
| `kv_cas_value` correct | Returns `false` on conflict, not error |
| `kv_keys` works | Lists keys with prefix/limit |
| `kv_scan` works | Cursor pagination through large sets |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Storage changes break other primitives | Test all primitives after storage changes |
| History iteration performance | Use iterator, not collect-all-then-filter |
| Retry loop infinite | Cap at 10 retries with exponential backoff |
| Cursor encoding | Use key string directly (simple, debuggable) |

---

## Implementation Order

```
1. Storage: VersionChain::history()
2. Storage: Storage::get_history() trait
3. Storage: ShardedStore::get_history() impl
4. Primitive: KVStore::get_at()
5. Primitive: KVStore::history()
6. Primitive: KVStore::keys()
7. Primitive: KVStore::scan()
8. Substrate: kv_get_at (wire to primitive)
9. Substrate: kv_history (wire to primitive)
10. Substrate: kv_incr (add retry)
11. Substrate: kv_cas_value (catch conflict)
12. Substrate: kv_keys (add trait + impl)
13. Substrate: kv_scan (add trait + impl)
14. Tests: Unignore and verify
```

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-23 | Initial plan |
