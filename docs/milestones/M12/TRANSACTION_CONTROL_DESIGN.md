# TransactionControl Implementation Design

> **Status**: Design Document
> **Date**: 2026-01-23
> **Complexity**: High
> **Estimated Effort**: 2-3 days

---

## Executive Summary

The `TransactionControl` trait is defined in `crates/api/src/substrate/transaction.rs` but has **no implementation** for `SubstrateImpl`. This document details what would be required to implement explicit transaction control (begin/commit/rollback) on top of the existing closure-based transaction system.

---

## Current State

### What Exists

**Trait Definition** (`crates/api/src/substrate/transaction.rs:111-178`):

```rust
pub trait TransactionControl {
    fn txn_begin(&self, options: Option<TxnOptions>) -> StrataResult<TxnId>;
    fn txn_commit(&self) -> StrataResult<Version>;
    fn txn_rollback(&self) -> StrataResult<()>;
    fn txn_info(&self) -> StrataResult<Option<TxnInfo>>;
    fn txn_is_active(&self) -> StrataResult<bool>;
}
```

**Database Transaction API** (`crates/engine/src/database.rs`):

```rust
// Closure-based transactions (current model)
pub fn transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T>
pub fn transaction_with_version<F, T>(&self, run_id: RunId, f: F) -> Result<(T, u64)>
pub fn transaction_with_retry<F, T>(&self, run_id: RunId, config: RetryConfig, f: F) -> Result<T>
```

### The Impedance Mismatch

| Aspect | Current Model | TransactionControl Model |
|--------|---------------|--------------------------|
| Scope | Single closure | Multiple separate calls |
| Lifetime | Closure duration | Explicit begin/commit |
| State | Stack-local | Persistent context |
| Rollback | Automatic on error | Explicit call |
| Cross-call | Not possible | Required |

The Database uses closure-based transactions where the transaction context exists only within the closure. The `TransactionControl` trait expects the transaction context to persist across multiple API calls.

---

## Design Options

### Option A: Thread-Local Transaction Context

Store active transaction state in thread-local storage.

**Pros:**
- Simple to implement
- No changes to Database layer
- Works with existing synchronous API

**Cons:**
- Doesn't work with async/await
- Can't share transactions across threads
- Memory leaks if not properly cleaned up

**Implementation Sketch:**

```rust
use std::cell::RefCell;

thread_local! {
    static ACTIVE_TXN: RefCell<Option<ActiveTransaction>> = RefCell::new(None);
}

struct ActiveTransaction {
    id: TxnId,
    run_id: RunId,
    started_at: u64,
    pending_writes: Vec<PendingWrite>,
    pending_deletes: Vec<Key>,
}

enum PendingWrite {
    KvPut { key: String, value: Value },
    KvDelete { key: String },
    EventAppend { stream: String, payload: Value },
    StateSet { cell: String, value: Value },
    // ... all other write operations
}
```

### Option B: Connection-Scoped Context

Add a `SubstrateConnection` type that holds transaction state.

**Pros:**
- Explicit lifetime management
- Could work with async (with proper design)
- Clear ownership

**Cons:**
- Breaking API change
- Users must manage connection lifetime
- More complex API surface

**Implementation Sketch:**

```rust
pub struct SubstrateConnection {
    substrate: SubstrateImpl,
    active_txn: Option<ActiveTransaction>,
}

impl SubstrateConnection {
    pub fn begin(&mut self, options: Option<TxnOptions>) -> StrataResult<TxnId> { ... }
    pub fn commit(&mut self) -> StrataResult<Version> { ... }
    pub fn rollback(&mut self) -> StrataResult<()> { ... }

    // All operations go through the connection
    pub fn kv_put(&mut self, run: &ApiRunId, key: &str, value: Value) -> StrataResult<Version> { ... }
}
```

### Option C: Transaction Handle Pattern

Return a transaction handle that operations can use.

**Pros:**
- Explicit transaction scope
- Type-safe (can't use wrong transaction)
- Works with async

**Cons:**
- Breaking API change
- Every operation needs transaction parameter
- More verbose API

**Implementation Sketch:**

```rust
pub struct Transaction {
    id: TxnId,
    substrate: SubstrateImpl,
    pending: RefCell<Vec<PendingWrite>>,
}

impl Transaction {
    pub fn kv_put(&self, run: &ApiRunId, key: &str, value: Value) -> StrataResult<()> { ... }
    pub fn commit(self) -> StrataResult<Version> { ... }
    pub fn rollback(self) -> StrataResult<()> { ... }
}

// Usage:
let txn = substrate.begin_transaction()?;
txn.kv_put(&run, "key1", value1)?;
txn.kv_put(&run, "key2", value2)?;
txn.commit()?;
```

### Recommendation: Option A (Thread-Local) for M11

For M11, implement Option A (thread-local) because:
1. Minimal API changes
2. Works with current synchronous model
3. Can be upgraded to Option B/C later
4. Matches common database driver patterns (JDBC, etc.)

Document the async limitation clearly.

---

## Detailed Implementation Plan

### Phase 1: Core Transaction State

**File:** `crates/api/src/substrate/transaction.rs`

```rust
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global transaction ID counter
static TXN_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Thread-local active transaction
thread_local! {
    static ACTIVE_TXN: RefCell<Option<ActiveTransaction>> = RefCell::new(None);
}

/// Pending write operation
#[derive(Debug, Clone)]
pub(crate) enum PendingOp {
    // KV operations
    KvPut { run: ApiRunId, key: String, value: Value },
    KvDelete { run: ApiRunId, key: String },

    // Event operations
    EventAppend { run: ApiRunId, stream: String, payload: Value },

    // State operations
    StateSet { run: ApiRunId, cell: String, value: Value },
    StateDelete { run: ApiRunId, cell: String },

    // JSON operations
    JsonSet { run: ApiRunId, doc_id: String, value: Value },
    JsonDelete { run: ApiRunId, doc_id: String },
    JsonPatch { run: ApiRunId, doc_id: String, path: String, value: Value },

    // Vector operations
    VectorUpsert { run: ApiRunId, collection: String, key: String, vector: Vec<f32>, metadata: Option<Value> },
    VectorDelete { run: ApiRunId, collection: String, key: String },

    // Trace operations
    TraceCreate { run: ApiRunId, parent: Option<String>, trace_type: String, content: Value, tags: Vec<String> },

    // Run operations
    RunCreate { run: ApiRunId, metadata: Option<Value> },
    RunClose { run: ApiRunId },
}

/// Active transaction state
struct ActiveTransaction {
    id: TxnId,
    options: TxnOptions,
    started_at: u64,
    pending_ops: Vec<PendingOp>,
    /// Tracks which runs are involved (for cross-run validation)
    involved_runs: std::collections::HashSet<ApiRunId>,
}

impl ActiveTransaction {
    fn new(id: TxnId, options: TxnOptions) -> Self {
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);

        Self {
            id,
            options,
            started_at,
            pending_ops: Vec::new(),
            involved_runs: std::collections::HashSet::new(),
        }
    }

    fn add_op(&mut self, op: PendingOp) {
        // Track involved run
        match &op {
            PendingOp::KvPut { run, .. } |
            PendingOp::KvDelete { run, .. } |
            PendingOp::EventAppend { run, .. } |
            PendingOp::StateSet { run, .. } |
            PendingOp::StateDelete { run, .. } => {
                self.involved_runs.insert(run.clone());
            }
            // ... other operations
            _ => {}
        }
        self.pending_ops.push(op);
    }

    fn operation_count(&self) -> u64 {
        self.pending_ops.len() as u64
    }
}
```

### Phase 2: TransactionControl Implementation

```rust
impl TransactionControl for SubstrateImpl {
    fn txn_begin(&self, options: Option<TxnOptions>) -> StrataResult<TxnId> {
        ACTIVE_TXN.with(|cell| {
            let mut active = cell.borrow_mut();
            if active.is_some() {
                return Err(StrataError::constraint_violation(
                    "Transaction already active in this thread"
                ));
            }

            let id = TxnId::new(TXN_ID_COUNTER.fetch_add(1, Ordering::SeqCst));
            *active = Some(ActiveTransaction::new(id, options.unwrap_or_default()));
            Ok(id)
        })
    }

    fn txn_commit(&self) -> StrataResult<Version> {
        ACTIVE_TXN.with(|cell| {
            let mut active = cell.borrow_mut();
            let txn = active.take().ok_or_else(|| {
                StrataError::constraint_violation("No active transaction")
            })?;

            // Execute all pending operations atomically
            self.execute_pending_ops(txn.pending_ops)
        })
    }

    fn txn_rollback(&self) -> StrataResult<()> {
        ACTIVE_TXN.with(|cell| {
            let mut active = cell.borrow_mut();
            if active.is_none() {
                return Err(StrataError::constraint_violation("No active transaction"));
            }
            *active = None; // Just discard pending ops
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
                operation_count: txn.operation_count(),
            }))
        })
    }

    fn txn_is_active(&self) -> StrataResult<bool> {
        ACTIVE_TXN.with(|cell| Ok(cell.borrow().is_some()))
    }
}
```

### Phase 3: Execute Pending Operations

```rust
impl SubstrateImpl {
    /// Execute all pending operations in a single database transaction
    fn execute_pending_ops(&self, ops: Vec<PendingOp>) -> StrataResult<Version> {
        if ops.is_empty() {
            return Ok(Version::Txn(0));
        }

        // Group operations by run
        let mut ops_by_run: HashMap<ApiRunId, Vec<&PendingOp>> = HashMap::new();
        for op in &ops {
            let run = op.run_id();
            ops_by_run.entry(run.clone()).or_default().push(op);
        }

        // For single-run transactions, execute directly
        if ops_by_run.len() == 1 {
            let (run, run_ops) = ops_by_run.into_iter().next().unwrap();
            return self.execute_single_run_ops(&run, run_ops);
        }

        // For cross-run transactions, need special handling
        // (This is complex - may need distributed transaction support)
        Err(StrataError::not_implemented(
            "Cross-run transactions not yet supported"
        ))
    }

    fn execute_single_run_ops(&self, run: &ApiRunId, ops: Vec<&PendingOp>) -> StrataResult<Version> {
        let run_id = run.to_run_id();

        let (_, version) = self.db.transaction_with_version(run_id, |txn| {
            for op in ops {
                match op {
                    PendingOp::KvPut { key, value, .. } => {
                        let storage_key = Key::new_kv(/* namespace */, key);
                        txn.put(storage_key, value.clone())?;
                    }
                    PendingOp::KvDelete { key, .. } => {
                        let storage_key = Key::new_kv(/* namespace */, key);
                        txn.delete(storage_key)?;
                    }
                    // ... handle all other operation types
                    _ => {}
                }
            }
            Ok(())
        }).map_err(convert_error)?;

        Ok(Version::Txn(version))
    }
}
```

### Phase 4: Modify All Primitive Operations

Every write operation needs to be modified to check for an active transaction.

**Pattern for each operation:**

```rust
// Before (auto-commit):
fn kv_put(&self, run: &ApiRunId, key: &str, value: Value) -> StrataResult<Version> {
    validate_key(key)?;
    let run_id = run.to_run_id();
    let version = self.kv().put(run_id, key, value).map_err(convert_error)?;
    Ok(Version::Txn(version))
}

// After (transaction-aware):
fn kv_put(&self, run: &ApiRunId, key: &str, value: Value) -> StrataResult<Version> {
    validate_key(key)?;

    // Check if in explicit transaction
    if self.txn_is_active()? {
        // Queue operation for later commit
        ACTIVE_TXN.with(|cell| {
            let mut active = cell.borrow_mut();
            if let Some(txn) = active.as_mut() {
                txn.add_op(PendingOp::KvPut {
                    run: run.clone(),
                    key: key.to_string(),
                    value: value.clone(),
                });
            }
        });
        // Return placeholder version (actual version assigned at commit)
        Ok(Version::Txn(0))
    } else {
        // Auto-commit mode (current behavior)
        let run_id = run.to_run_id();
        let version = self.kv().put(run_id, key, value).map_err(convert_error)?;
        Ok(Version::Txn(version))
    }
}
```

---

## Files Requiring Modification

| File | Changes Required |
|------|------------------|
| `crates/api/src/substrate/transaction.rs` | Add implementation, PendingOp enum, thread-local state |
| `crates/api/src/substrate/kv.rs` | Modify all 15+ write operations |
| `crates/api/src/substrate/event.rs` | Modify `event_append` |
| `crates/api/src/substrate/state.rs` | Modify `state_set`, `state_delete`, `state_transition*` |
| `crates/api/src/substrate/json.rs` | Modify `json_set`, `json_delete`, `json_patch*` |
| `crates/api/src/substrate/vector.rs` | Modify `vector_upsert`, `vector_delete`, `vector_create_collection` |
| `crates/api/src/substrate/trace.rs` | Modify `trace_create`, `trace_create_with_id`, `trace_update_tags` |
| `crates/api/src/substrate/run.rs` | Modify `run_create`, `run_close`, `run_set_*` |
| `crates/api/src/substrate/impl_.rs` | Add helper methods |

**Estimated: ~50 methods need modification**

---

## Read-Your-Writes Semantics

A key decision: should reads within a transaction see pending writes?

### Option 1: No Read-Your-Writes (Simpler)

Reads always go to committed state. Pending writes only visible after commit.

```rust
// Transaction started
txn.kv_put(&run, "key", Value::Int(1));
let val = txn.kv_get(&run, "key"); // Returns None (not committed yet)
txn.commit();
let val = txn.kv_get(&run, "key"); // Returns Some(1)
```

**Pros:** Much simpler implementation
**Cons:** Surprising behavior, can't build on previous writes

### Option 2: Read-Your-Writes (Complex)

Reads check pending writes first, then fall back to committed state.

```rust
fn kv_get(&self, run: &ApiRunId, key: &str) -> StrataResult<Option<Versioned<Value>>> {
    // First check pending writes
    if let Some(pending) = self.get_pending_write(run, key) {
        return Ok(Some(Versioned {
            value: pending.clone(),
            version: Version::Txn(0), // Uncommitted
            timestamp: 0,
        }));
    }

    // Fall back to committed state
    // ... existing implementation
}
```

**Pros:** Intuitive behavior
**Cons:** Complex implementation, need to track all pending state

### Recommendation

Start with **Option 1** (no read-your-writes) for simplicity. Document the limitation clearly. Add read-your-writes in a future iteration if needed.

---

## Testing Requirements

### Unit Tests

```rust
#[test]
fn test_txn_begin_commit() {
    let substrate = create_test_substrate();
    let run = ApiRunId::default();

    let txn_id = substrate.txn_begin(None).unwrap();
    substrate.kv_put(&run, "key", Value::Int(1)).unwrap();
    let version = substrate.txn_commit().unwrap();

    // Verify committed
    let val = substrate.kv_get(&run, "key").unwrap().unwrap();
    assert_eq!(val.value, Value::Int(1));
}

#[test]
fn test_txn_rollback() {
    let substrate = create_test_substrate();
    let run = ApiRunId::default();

    substrate.txn_begin(None).unwrap();
    substrate.kv_put(&run, "key", Value::Int(1)).unwrap();
    substrate.txn_rollback().unwrap();

    // Verify not committed
    let val = substrate.kv_get(&run, "key").unwrap();
    assert!(val.is_none());
}

#[test]
fn test_txn_nested_error() {
    let substrate = create_test_substrate();

    substrate.txn_begin(None).unwrap();
    let result = substrate.txn_begin(None);
    assert!(result.is_err()); // Can't nest transactions
}

#[test]
fn test_txn_commit_without_begin() {
    let substrate = create_test_substrate();

    let result = substrate.txn_commit();
    assert!(result.is_err());
}

#[test]
fn test_txn_multi_primitive() {
    let substrate = create_test_substrate();
    let run = ApiRunId::default();

    substrate.txn_begin(None).unwrap();
    substrate.kv_put(&run, "kv_key", Value::Int(1)).unwrap();
    substrate.state_set(&run, "state_cell", Value::Int(2)).unwrap();
    substrate.txn_commit().unwrap();

    // Both should be committed
    let kv = substrate.kv_get(&run, "kv_key").unwrap().unwrap();
    let state = substrate.state_get(&run, "state_cell").unwrap().unwrap();
    assert_eq!(kv.value, Value::Int(1));
    assert_eq!(state.value, Value::Int(2));
}

#[test]
fn test_txn_info() {
    let substrate = create_test_substrate();

    assert!(substrate.txn_info().unwrap().is_none());

    let txn_id = substrate.txn_begin(None).unwrap();

    let info = substrate.txn_info().unwrap().unwrap();
    assert_eq!(info.id, txn_id);
    assert_eq!(info.status, TxnStatus::Active);
    assert_eq!(info.operation_count, 0);
}

#[test]
fn test_txn_atomicity_on_error() {
    let substrate = create_test_substrate();
    let run = ApiRunId::default();

    substrate.kv_put(&run, "existing", Value::Int(0)).unwrap();

    substrate.txn_begin(None).unwrap();
    substrate.kv_put(&run, "existing", Value::Int(1)).unwrap();
    substrate.kv_put(&run, "new_key", Value::Int(2)).unwrap();
    // Simulate error during commit (e.g., constraint violation)
    // ... how to trigger?

    // If commit fails, neither write should be visible
}
```

### Integration Tests

- Transaction across KV + Event + State
- Transaction with concurrent access
- Transaction timeout behavior
- Thread isolation (different threads have different transactions)

---

## Limitations to Document

1. **Not async-compatible**: Thread-local storage doesn't work with async/await. If you `await` within a transaction, you may resume on a different thread.

2. **No nested transactions**: Only one transaction per thread at a time.

3. **No read-your-writes**: Reads don't see pending writes until commit.

4. **Single-run only** (initially): Cross-run transactions not supported.

5. **No savepoints** (initially): Can't partially rollback.

---

## Migration Path

### Phase 1: Basic Implementation
- Thread-local context
- Single-run transactions
- No read-your-writes
- All primitives updated

### Phase 2: Enhanced Features
- Read-your-writes support
- Transaction timeout enforcement
- Better error messages

### Phase 3: Advanced
- Savepoints (TransactionSavepoint trait)
- Cross-run transactions
- Async-compatible design (Option B or C)

---

## Alternatives to Full Implementation

If full TransactionControl is too complex for M11, consider:

### Alternative 1: Batch Operations Only

Expand `kv_batch_*` pattern to all primitives:

```rust
fn multi_write(&self, ops: Vec<WriteOp>) -> StrataResult<Version>;

enum WriteOp {
    KvPut(ApiRunId, String, Value),
    KvDelete(ApiRunId, String),
    StateSet(ApiRunId, String, Value),
    // ...
}
```

### Alternative 2: Closure-Based Transactions

Expose the closure pattern at the API level:

```rust
fn with_transaction<F, T>(&self, run: &ApiRunId, f: F) -> StrataResult<T>
where
    F: FnOnce(&TransactionContext) -> StrataResult<T>;

// Usage:
substrate.with_transaction(&run, |txn| {
    txn.kv_put("key1", value1)?;
    txn.kv_put("key2", value2)?;
    Ok(())
})?;
```

This is simpler and matches the underlying Database API.

---

## Recommendation

For M11, **do not implement TransactionControl**. Instead:

1. Document that auto-commit is the current behavior
2. Expand batch operations for atomic multi-key writes
3. Consider closure-based transactions as a simpler alternative
4. Plan full TransactionControl for a future milestone

The current auto-commit model works for most use cases, and the `kv_batch_*` operations provide atomic multi-key writes within a single primitive.

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-23 | Initial design document |
