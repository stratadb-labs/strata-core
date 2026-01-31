# Error Handling Guide

All StrataDB operations return `Result<T, Error>`. The `Error` enum has structured variants so you can match on specific error conditions.

## Using the ? Operator

The recommended pattern uses Rust's `?` operator:

```rust
fn do_work(db: &Strata) -> stratadb::Result<()> {
    db.kv_put("key", "value")?;
    let val = db.kv_get("key")?;
    Ok(())
}
```

## Error Categories

### Not Found Errors

Returned when an entity doesn't exist.

| Variant | When |
|---------|------|
| `KeyNotFound { key }` | KV key not found |
| `BranchNotFound { branch }` | Branch doesn't exist |
| `CollectionNotFound { collection }` | Vector collection doesn't exist |
| `StreamNotFound { stream }` | Event stream not found |
| `CellNotFound { cell }` | State cell not found |
| `DocumentNotFound { key }` | JSON document not found |

```rust
match db.kv_get("missing") {
    Ok(None) => println!("Key not found (normal)"),
    Ok(Some(v)) => println!("Found: {:?}", v),
    Err(e) => println!("Error: {}", e),
}
```

Note: `kv_get` returns `Ok(None)` for missing keys, not an error. The `KeyNotFound` error is used in contexts where the key is required.

### Validation Errors

Returned when input is malformed.

| Variant | When |
|---------|------|
| `InvalidKey { reason }` | Key format is invalid |
| `InvalidPath { reason }` | JSON path is malformed |
| `InvalidInput { reason }` | General input validation failure |
| `WrongType { expected, actual }` | Type mismatch |

### Concurrency Errors

Returned when concurrent operations conflict.

| Variant | When |
|---------|------|
| `VersionConflict { expected, actual }` | CAS version doesn't match |
| `TransitionFailed { expected, actual }` | State transition condition not met |
| `Conflict { reason }` | General conflict |

### State Errors

Returned when an operation violates state constraints.

| Variant | When |
|---------|------|
| `BranchClosed { branch }` | Operation on a closed branch |
| `BranchExists { branch }` | Creating a branch that already exists |
| `CollectionExists { collection }` | Creating a collection that already exists |

### Constraint Errors

Returned when limits or constraints are violated.

| Variant | When |
|---------|------|
| `DimensionMismatch { expected, actual }` | Vector dimension doesn't match collection |
| `ConstraintViolation { reason }` | General constraint violation (e.g., deleting default branch) |
| `HistoryTrimmed { requested, earliest }` | Requested version was removed by retention |
| `Overflow { reason }` | Numeric overflow |

### Transaction Errors

Returned during transaction lifecycle issues.

| Variant | When |
|---------|------|
| `TransactionNotActive` | Commit/rollback without an active transaction |
| `TransactionAlreadyActive` | Begin while a transaction is already open |
| `TransactionConflict { reason }` | Commit-time validation failure |

### System Errors

Returned for infrastructure-level failures.

| Variant | When |
|---------|------|
| `Io { reason }` | File I/O failure |
| `Serialization { reason }` | Data serialization/deserialization failure |
| `Internal { reason }` | Bug or invariant violation |
| `NotImplemented { feature, reason }` | Feature not yet available |

## Common Patterns

### Match on Specific Errors

```rust
use stratadb::Error;

match db.create_branch("my-branch") {
    Ok(()) => println!("Created"),
    Err(Error::BranchExists { branch }) => println!("Branch {} already exists", branch),
    Err(e) => return Err(e),
}
```

### Transaction Retry

```rust
loop {
    session.execute(Command::TxnBegin { branch: None, options: None })?;
    // ... operations ...
    match session.execute(Command::TxnCommit) {
        Ok(_) => break,
        Err(Error::TransactionConflict { .. }) => continue,
        Err(e) => return Err(e),
    }
}
```

### Idempotent Operations

Use `state_init` and check for `BranchExists` to write idempotent code:

```rust
// Create branch only if it doesn't exist
match db.create_branch("session-001") {
    Ok(()) => {},
    Err(Error::BranchExists { .. }) => {},  // Already exists — fine
    Err(e) => return Err(e),
}
```

## Next

- [Error Reference](../reference/error-reference.md) — complete error specification
- [Sessions and Transactions](sessions-and-transactions.md) — transaction patterns
