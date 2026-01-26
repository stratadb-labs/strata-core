# Strata Error Handling Guidelines

This document describes the error handling patterns and best practices for the in-mem Strata database.

## Overview

Strata uses a unified error type `StrataError` for all API operations. This provides consistent error handling across all primitives (KV, EventLog, StateCell, TraceStore, JsonStore, Vector, Run).

## Error Categories

### Not Found Errors

Returned when an entity doesn't exist.

| Variant | Description |
|---------|-------------|
| `NotFound` | Entity (key, document, event, etc.) not found |
| `RunNotFound` | Run doesn't exist |
| `PathNotFound` | JSON path doesn't exist in document |

**Handling Pattern:**

```rust
match db.kv().get(&run_id, "key")? {
    Some(value) => { /* use value */ }
    None => { /* handle missing */ }
}

// Or check first
if !db.kv().exists(&run_id, "key")? {
    // Create it
}
```

### Conflict Errors

Returned when there's a version or write conflict. These are typically **retryable**.

| Variant | Description |
|---------|-------------|
| `VersionConflict` | CAS operation failed - version mismatch |
| `WriteConflict` | Concurrent write detected in transaction |

**Handling Pattern:**

```rust
// Compare-and-swap pattern
loop {
    let current = db.state().read(&run_id, "counter")?.unwrap();
    let new_value = current.value.as_i64().unwrap() + 1;

    match db.state().cas(&run_id, "counter", current.version, Value::from(new_value)) {
        Ok(new_version) => break,
        Err(StrataError::VersionConflict { .. }) => continue, // Retry
        Err(e) => return Err(e),
    }
}
```

### Transaction Errors

Returned for transaction-level failures.

| Variant | Description |
|---------|-------------|
| `TransactionAborted` | Transaction aborted (conflict, timeout, or other) |
| `TransactionTimeout` | Transaction exceeded max duration |
| `TransactionNotActive` | Operation on already-committed/rolled-back transaction |

**Handling Pattern:**

```rust
match db.transaction(&run_id, |txn| {
    txn.kv_put("key", value)?;
    Ok(())
}) {
    Ok(_) => { /* committed */ }
    Err(StrataError::TransactionAborted { reason }) => {
        // Log and possibly retry
    }
    Err(StrataError::WriteConflict { entity_ref }) => {
        // Concurrent modification - retry
    }
    Err(e) => {
        // Other error - don't retry
    }
}
```

### Validation Errors

Returned for invalid input. **Don't retry** - fix the input.

| Variant | Description |
|---------|-------------|
| `InvalidOperation` | Operation not valid for current state |
| `InvalidInput` | Invalid parameters |
| `DimensionMismatch` | Vector dimension doesn't match collection |

**Handling Pattern:**

```rust
match result {
    Err(StrataError::DimensionMismatch { expected, got }) => {
        panic!("Bug: embedding has wrong dimension");
    }
    Err(StrataError::InvalidInput { message }) => {
        // User error - report to user
    }
    // ...
}
```

### Storage Errors

Returned for low-level storage failures.

| Variant | Description |
|---------|-------------|
| `Storage` | Storage operation failed |
| `Serialization` | Serialize/deserialize failed |
| `Corruption` | Data integrity check failed (**serious**) |

### Resource Errors

Returned when resource limits are exceeded.

| Variant | Description |
|---------|-------------|
| `CapacityExceeded` | Resource limit exceeded |
| `BudgetExceeded` | Computational budget exceeded |

### Internal Errors

| Variant | Description |
|---------|-------------|
| `Internal` | Unexpected internal error (**serious**) |

## Classification Methods

`StrataError` provides methods to classify errors:

```rust
// Check error type
if error.is_not_found() { ... }
if error.is_conflict() { ... }
if error.is_transaction_error() { ... }
if error.is_validation_error() { ... }
if error.is_storage_error() { ... }
if error.is_resource_error() { ... }

// Retry logic
if error.is_retryable() {
    // VersionConflict, WriteConflict, TransactionAborted
}

// Serious errors (log and alert)
if error.is_serious() {
    // Corruption, Internal
}
```

## Accessing Context

`StrataError` provides context about the error:

```rust
// Get the entity that caused the error
if let Some(entity_ref) = error.entity_ref() {
    println!("Error on entity: {}", entity_ref);
    // e.g., "kv://abc123/config"
}

// Get the run ID
if let Some(run_id) = error.run_id() {
    println!("Error in run: {}", run_id);
}
```

## Best Practices

### 1. Use the `?` Operator

Let errors propagate naturally:

```rust
fn process_data(run_id: RunId) -> StrataResult<()> {
    let value = db.kv().get(&run_id, "key")?;
    let events = db.events().read_range(&run_id, 0, 100)?;
    Ok(())
}
```

### 2. Match Specific Variants

When you need to handle specific cases:

```rust
match db.kv().get(&run_id, "key") {
    Ok(Some(value)) => { /* success */ }
    Ok(None) => { /* not found - create default */ }
    Err(StrataError::NotFound { .. }) => { /* explicit not found */ }
    Err(e) => return Err(e),
}
```

### 3. Check `is_retryable()` Before Retry

```rust
fn with_retry<T, F: Fn() -> StrataResult<T>>(f: F, max_retries: usize) -> StrataResult<T> {
    let mut attempts = 0;
    loop {
        match f() {
            Ok(result) => return Ok(result),
            Err(e) if e.is_retryable() && attempts < max_retries => {
                attempts += 1;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}
```

### 4. Log Serious Errors

```rust
if error.is_serious() {
    log::error!("SERIOUS ERROR: {}", error);
    // Consider alerting/paging
}
```

### 5. Preserve Context When Wrapping

When creating custom errors that wrap `StrataError`, preserve the `EntityRef`:

```rust
#[derive(Debug, Error)]
pub enum MyError {
    #[error("Failed to process {entity_ref}: {source}")]
    ProcessFailed {
        entity_ref: EntityRef,
        #[source]
        source: StrataError,
    },
}
```

## Error Conversion

`StrataError` can be converted from various error types:

```rust
// Standard library
impl From<std::io::Error> for StrataError
impl From<serde_json::Error> for StrataError
impl From<bincode::Error> for StrataError

// Primitive errors
impl From<VectorError> for StrataError
impl From<RunError> for StrataError
impl From<CommitError> for StrataError
```

## Type Alias

Use `StrataResult<T>` as the return type:

```rust
use in_mem_core::{StrataError, StrataResult};

fn my_function() -> StrataResult<String> {
    // ...
}
```

## Error Display

All errors have meaningful display messages:

```
not found: kv://abc123/missing-key
version conflict on state://abc123/counter: expected cnt:5, got cnt:6
transaction aborted: Conflict on key 'shared-key'
transaction timeout after 5000ms
dimension mismatch: expected 384, got 768
```

## Migration from Legacy Errors

If you're migrating from the legacy `Error` type, you can convert:

```rust
// Automatic conversion
let legacy_error: Error = /* ... */;
let strata_error: StrataError = legacy_error.into();
```

The mapping is:

| Legacy Error | StrataError |
|--------------|-------------|
| `IoError` | `Storage` |
| `SerializationError` | `Serialization` |
| `KeyNotFound` | `NotFound` |
| `VersionMismatch` | `VersionConflict` |
| `Corruption` | `Corruption` |
| `InvalidOperation` | `InvalidInput` |
| `TransactionAborted` | `TransactionAborted` |
| `StorageError` | `Storage` |
| `InvalidState` | `InvalidInput` |
| `TransactionConflict` | `WriteConflict` |
| `TransactionTimeout` | `TransactionTimeout` |
