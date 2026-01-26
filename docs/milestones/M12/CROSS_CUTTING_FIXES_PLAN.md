# Cross-Cutting Fixes Plan

> **Status**: Planning
> **Date**: 2026-01-23
> **Scope**: Key Validation + TransactionControl + Retention

---

## Executive Summary

This plan addresses three cross-cutting concerns from `FOUNDATIONAL_CAPABILITIES_AUDIT.md`:

1. **Key Validation Enhancement** - Add max length check to existing validation
2. **TransactionControl** - Implement explicit txn_begin/commit/rollback API
3. **Retention System** - Wire retention policies to storage with basic GC

---

## Part 1: Key Validation Enhancement

### Current State

Key validation in `crates/api/src/substrate/impl_.rs:236` already checks:
- Non-empty ✓
- No NUL bytes ✓
- No `_strata/` prefix ✓

**Missing**: Max key length validation (per `crates/core/src/limits.rs`, default 4KB)

### Required Changes

#### File: `crates/api/src/substrate/impl_.rs`

```rust
use strata_core::limits::Limits;

/// Validate a KV key according to the contract:
/// - Non-empty
/// - No NUL bytes
/// - Not starting with `_strata/` (reserved prefix)
/// - Not exceeding max key length (default 4KB)
///
/// Returns an error if the key is invalid.
pub(crate) fn validate_key(key: &str) -> StrataResult<()> {
    if key.is_empty() {
        return Err(StrataError::invalid_input("Key must not be empty"));
    }
    if key.contains('\0') {
        return Err(StrataError::invalid_input("Key must not contain NUL bytes"));
    }
    if key.starts_with(RESERVED_KEY_PREFIX) {
        return Err(StrataError::invalid_input(
            format!("Key must not start with reserved prefix '{}'", RESERVED_KEY_PREFIX)
        ));
    }
    // Add max length check
    let limits = Limits::default();
    if key.len() > limits.max_key_bytes {
        return Err(StrataError::invalid_input(
            format!("Key exceeds maximum length of {} bytes", limits.max_key_bytes)
        ));
    }
    Ok(())
}
```

### Tests to Add

```rust
#[test]
fn test_kv_key_max_length() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Key at max length should work
    let max_key = "k".repeat(4096);
    assert!(substrate.kv_put(&run, &max_key, Value::Int(1)).is_ok());

    // Key over max length should fail
    let over_key = "k".repeat(4097);
    let result = substrate.kv_put(&run, &over_key, Value::Int(1));
    assert!(result.is_err());
}
```

### Acceptance Criteria

- [ ] Keys > 4KB return `InvalidInput` error
- [ ] Keys exactly 4KB succeed
- [ ] Existing validation tests still pass

---

## Part 2: TransactionControl Implementation

### Current State

- `TransactionControl` trait is defined in `crates/api/src/substrate/transaction.rs`
- No implementation exists for `SubstrateImpl`
- Database uses closure-based transactions (`db.transaction(|txn| { ... })`)

### Design Decision: Thread-Local Transaction Context

The Database API uses closure-based transactions, but the `TransactionControl` trait wants explicit begin/commit/rollback. We need a bridge.

**Approach**: Use `thread_local!` storage for the active transaction context.

### Required Changes

#### File: `crates/api/src/substrate/transaction.rs`

Add after the trait definition:

```rust
use super::impl_::SubstrateImpl;
use std::cell::RefCell;
use strata_concurrency::TransactionContext;

/// Thread-local storage for active transaction
thread_local! {
    static ACTIVE_TXN: RefCell<Option<ActiveTransaction>> = RefCell::new(None);
}

/// Active transaction state
struct ActiveTransaction {
    id: TxnId,
    run_id: crate::substrate::types::ApiRunId,
    started_at: u64,
    /// Pending operations (key, value) to be committed
    pending_puts: Vec<(strata_core::Key, strata_core::Value)>,
    pending_deletes: Vec<strata_core::Key>,
    operation_count: u64,
}

impl TransactionControl for SubstrateImpl {
    fn txn_begin(&self, options: Option<TxnOptions>) -> StrataResult<TxnId> {
        ACTIVE_TXN.with(|cell| {
            let mut active = cell.borrow_mut();
            if active.is_some() {
                return Err(StrataError::constraint_violation(
                    "Transaction already active in this context"
                ));
            }

            let id = TxnId::new(self.next_txn_id());
            let started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_micros() as u64)
                .unwrap_or(0);

            *active = Some(ActiveTransaction {
                id,
                run_id: crate::substrate::types::ApiRunId::default(), // Will be set on first op
                started_at,
                pending_puts: Vec::new(),
                pending_deletes: Vec::new(),
                operation_count: 0,
            });

            Ok(id)
        })
    }

    fn txn_commit(&self) -> StrataResult<Version> {
        ACTIVE_TXN.with(|cell| {
            let mut active = cell.borrow_mut();
            let txn = active.take().ok_or_else(|| {
                StrataError::constraint_violation("No active transaction")
            })?;

            // Execute all pending operations in a single database transaction
            let run_id = strata_core::RunId::from(txn.run_id.as_str());
            let (_, version) = self.db.transaction_with_version(run_id, |db_txn| {
                for (key, value) in txn.pending_puts {
                    db_txn.put(key, value)?;
                }
                for key in txn.pending_deletes {
                    db_txn.delete(&key)?;
                }
                Ok(())
            }).map_err(convert_error)?;

            Ok(Version::Txn(version))
        })
    }

    fn txn_rollback(&self) -> StrataResult<()> {
        ACTIVE_TXN.with(|cell| {
            let mut active = cell.borrow_mut();
            if active.is_none() {
                return Err(StrataError::constraint_violation("No active transaction"));
            }
            *active = None;
            Ok(())
        })
    }

    fn txn_info(&self) -> StrataResult<Option<TxnInfo>> {
        ACTIVE_TXN.with(|cell| {
            let active = cell.borrow();
            Ok(active.as_ref().map(|txn| TxnInfo {
                id: txn.id,
                status: TxnStatus::Active,
                started_at: txn.started_at,
                completed_at: None,
                operation_count: txn.operation_count,
            }))
        })
    }

    fn txn_is_active(&self) -> StrataResult<bool> {
        ACTIVE_TXN.with(|cell| {
            Ok(cell.borrow().is_some())
        })
    }
}
```

### Integration with KV Operations

KV operations need to check if a transaction is active and either:
1. Queue operations in the pending list (if in explicit txn)
2. Execute immediately with auto-commit (if not in explicit txn)

This requires modifying `kv_put`, `kv_delete` etc. to check `txn_is_active()` first.

### Tests to Add

```rust
#[test]
fn test_explicit_transaction_commit() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Begin transaction
    let txn_id = substrate.txn_begin(None).unwrap();
    assert!(substrate.txn_is_active().unwrap());

    // Operations within transaction
    substrate.kv_put(&run, "txn:a", Value::Int(1)).unwrap();
    substrate.kv_put(&run, "txn:b", Value::Int(2)).unwrap();

    // Not visible until commit
    // (if we support read-your-writes this would differ)

    // Commit
    let version = substrate.txn_commit().unwrap();
    assert!(!substrate.txn_is_active().unwrap());

    // Now visible
    let a = substrate.kv_get(&run, "txn:a").unwrap().unwrap();
    assert_eq!(a.value, Value::Int(1));
}

#[test]
fn test_explicit_transaction_rollback() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Begin transaction
    substrate.txn_begin(None).unwrap();
    substrate.kv_put(&run, "rollback:key", Value::Int(1)).unwrap();

    // Rollback
    substrate.txn_rollback().unwrap();
    assert!(!substrate.txn_is_active().unwrap());

    // Key should not exist
    let result = substrate.kv_get(&run, "rollback:key").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_nested_transaction_error() {
    let (_, substrate) = quick_setup();

    substrate.txn_begin(None).unwrap();
    let result = substrate.txn_begin(None);
    assert!(result.is_err()); // Already active
}
```

### Acceptance Criteria

- [ ] `txn_begin()` starts a transaction context
- [ ] `txn_commit()` atomically applies all pending operations
- [ ] `txn_rollback()` discards all pending operations
- [ ] `txn_info()` returns current transaction state
- [ ] Nested `txn_begin()` returns error
- [ ] Commit/rollback without begin returns error

### Complexity Note

**This is a significant undertaking.** Full integration requires modifying ALL primitive operations to check for active transactions and queue operations accordingly.

**Recommendation**: Implement TransactionControl as a P1, not P0. The current auto-commit model works for most use cases.

---

## Part 3: Retention System

### Current State

- `RetentionSubstrate` trait is defined with `retention_get`, `retention_set`, `retention_clear`
- Implementation is completely stubbed (returns `Ok(None)`, `Ok(0)`, `Ok(false)`)
- No actual storage or enforcement of policies

### Design

Retention policies should be stored in a system namespace and enforced:
1. **Storage**: Store policy in `_system/retention_policy/{run_id}`
2. **Read**: Load policy from storage on `retention_get`
3. **Write**: Persist policy on `retention_set`
4. **Enforcement**: GC thread checks policies and trims versions (deferred)

### Required Changes

#### File: `crates/api/src/substrate/retention.rs`

Replace the stubbed implementation:

```rust
use super::impl_::{SubstrateImpl, convert_error};
use strata_core::{Key, Value, RunId};

/// System namespace key for retention policy
fn retention_key(run: &ApiRunId) -> Key {
    Key::new_system(&format!("retention_policy/{}", run.as_str()))
}

impl RetentionSubstrate for SubstrateImpl {
    fn retention_get(&self, run: &ApiRunId) -> StrataResult<Option<RetentionVersion>> {
        let key = retention_key(run);
        let run_id = RunId::system();

        let result = self.db.transaction(run_id, |txn| {
            txn.get(&key)
        }).map_err(convert_error)?;

        match result {
            Some(value) => {
                // Deserialize RetentionVersion from stored Value
                let rv = deserialize_retention(&value)?;
                Ok(Some(rv))
            }
            None => Ok(None),
        }
    }

    fn retention_set(&self, run: &ApiRunId, policy: RetentionPolicy) -> StrataResult<u64> {
        let key = retention_key(run);
        let run_id = RunId::system();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        let (_, version) = self.db.transaction_with_version(run_id, |txn| {
            let rv = RetentionVersion {
                policy: policy.clone(),
                version: 0, // Will be set to commit version
                timestamp,
            };
            let value = serialize_retention(&rv)?;
            txn.put(key.clone(), value)?;
            Ok(())
        }).map_err(convert_error)?;

        Ok(version)
    }

    fn retention_clear(&self, run: &ApiRunId) -> StrataResult<bool> {
        let key = retention_key(run);
        let run_id = RunId::system();

        let existed = self.db.transaction(run_id, |txn| {
            let existed = txn.get(&key)?.is_some();
            if existed {
                txn.delete(&key)?;
            }
            Ok(existed)
        }).map_err(convert_error)?;

        Ok(existed)
    }
}

/// Serialize RetentionVersion to Value
fn serialize_retention(rv: &RetentionVersion) -> StrataResult<Value> {
    // Store as Object with policy, version, timestamp
    let policy_str = match &rv.policy {
        RetentionPolicy::KeepAll => "keep_all".to_string(),
        RetentionPolicy::KeepLast(n) => format!("keep_last:{}", n),
        RetentionPolicy::KeepFor(d) => format!("keep_for:{}", d.as_secs()),
        RetentionPolicy::Composite(policies) => {
            // Simplified: just store first policy
            "composite".to_string()
        }
    };

    let mut obj = std::collections::HashMap::new();
    obj.insert("policy".to_string(), Value::String(policy_str));
    obj.insert("version".to_string(), Value::Int(rv.version as i64));
    obj.insert("timestamp".to_string(), Value::Int(rv.timestamp as i64));

    Ok(Value::Object(obj))
}

/// Deserialize RetentionVersion from Value
fn deserialize_retention(value: &Value) -> StrataResult<RetentionVersion> {
    let obj = match value {
        Value::Object(o) => o,
        _ => return Err(StrataError::invalid_input("Expected object for retention")),
    };

    let policy_str = match obj.get("policy") {
        Some(Value::String(s)) => s,
        _ => return Err(StrataError::invalid_input("Missing policy field")),
    };

    let policy = if policy_str == "keep_all" {
        RetentionPolicy::KeepAll
    } else if policy_str.starts_with("keep_last:") {
        let n: usize = policy_str[10..].parse().map_err(|_| {
            StrataError::invalid_input("Invalid keep_last value")
        })?;
        RetentionPolicy::KeepLast(n)
    } else if policy_str.starts_with("keep_for:") {
        let secs: u64 = policy_str[9..].parse().map_err(|_| {
            StrataError::invalid_input("Invalid keep_for value")
        })?;
        RetentionPolicy::KeepFor(std::time::Duration::from_secs(secs))
    } else {
        RetentionPolicy::KeepAll // fallback
    };

    let version = match obj.get("version") {
        Some(Value::Int(n)) => *n as u64,
        _ => 0,
    };

    let timestamp = match obj.get("timestamp") {
        Some(Value::Int(n)) => *n as u64,
        _ => 0,
    };

    Ok(RetentionVersion { policy, version, timestamp })
}
```

### Tests to Add

```rust
#[test]
fn test_retention_set_and_get() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Initially no policy
    let policy = substrate.retention_get(&run).unwrap();
    assert!(policy.is_none());

    // Set policy
    let version = substrate.retention_set(&run, RetentionPolicy::KeepLast(100)).unwrap();
    assert!(version > 0);

    // Get policy
    let policy = substrate.retention_get(&run).unwrap().unwrap();
    assert!(matches!(policy.policy, RetentionPolicy::KeepLast(100)));
}

#[test]
fn test_retention_clear() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Set then clear
    substrate.retention_set(&run, RetentionPolicy::KeepLast(10)).unwrap();
    let cleared = substrate.retention_clear(&run).unwrap();
    assert!(cleared);

    // Verify cleared
    let policy = substrate.retention_get(&run).unwrap();
    assert!(policy.is_none());

    // Clear again returns false
    let cleared = substrate.retention_clear(&run).unwrap();
    assert!(!cleared);
}

#[test]
fn test_retention_cross_mode() {
    test_across_modes("retention_cross_mode", |db| {
        let substrate = create_substrate(db);
        let run = ApiRunId::default();

        substrate.retention_set(&run, RetentionPolicy::KeepLast(50)).unwrap();
        let policy = substrate.retention_get(&run).unwrap();

        match policy {
            Some(rv) => matches!(rv.policy, RetentionPolicy::KeepLast(50)),
            None => false,
        }
    });
}
```

### Acceptance Criteria

- [ ] `retention_set()` persists policy to storage
- [ ] `retention_get()` retrieves policy from storage
- [ ] `retention_clear()` removes policy from storage
- [ ] Policies survive database restart (durability)
- [ ] Cross-mode equivalence (in-memory, buffered, strict)

### What This Does NOT Include

- **GC Enforcement**: Actually trimming versions based on policy (separate milestone)
- **Per-Key Retention**: Only run-level policies (per M11 scope)
- **`HistoryTrimmed` Error**: Returning correct error when history is unavailable

---

## Implementation Order

### Phase 1: Key Validation (P0) - ~30 mins
1. Add max length check to `validate_key()`
2. Add test for max length
3. Commit

### Phase 2: Retention Storage (P0) - ~1 hour
1. Implement `retention_get`, `retention_set`, `retention_clear`
2. Add serialization/deserialization helpers
3. Add tests
4. Commit

### Phase 3: TransactionControl (P1) - ~2-3 hours
1. Implement basic `txn_begin`, `txn_commit`, `txn_rollback`
2. Add thread-local transaction context
3. Modify KV operations to check for active transaction
4. Add tests
5. Commit

**Note**: Phase 3 is optional for M11. The auto-commit model works for most use cases, and full TransactionControl integration requires touching all primitives.

---

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Thread-local transactions don't work with async | Document limitation; async requires different approach |
| Retention GC could be expensive | Defer actual GC to separate milestone |
| TransactionControl is complex | Implement as P1, not P0 |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-23 | Initial plan |
