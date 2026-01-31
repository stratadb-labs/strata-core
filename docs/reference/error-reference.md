# Error Reference

Complete specification of every `Error` variant in StrataDB.

## Error Enum

```rust
pub enum Error {
    // Not Found
    KeyNotFound { key: String },
    BranchNotFound { branch: String },
    CollectionNotFound { collection: String },
    StreamNotFound { stream: String },
    CellNotFound { cell: String },
    DocumentNotFound { key: String },

    // Type
    WrongType { expected: String, actual: String },

    // Validation
    InvalidKey { reason: String },
    InvalidPath { reason: String },
    InvalidInput { reason: String },

    // Concurrency
    VersionConflict { expected: u64, actual: u64 },
    TransitionFailed { expected: String, actual: String },
    Conflict { reason: String },

    // State
    BranchClosed { branch: String },
    BranchExists { branch: String },
    CollectionExists { collection: String },

    // Constraint
    DimensionMismatch { expected: usize, actual: usize },
    ConstraintViolation { reason: String },
    HistoryTrimmed { requested: u64, earliest: u64 },
    Overflow { reason: String },

    // Transaction
    TransactionNotActive,
    TransactionAlreadyActive,
    TransactionConflict { reason: String },

    // System
    Io { reason: String },
    Serialization { reason: String },
    Internal { reason: String },
    NotImplemented { feature: String, reason: String },
}
```

## Not Found Errors

### `KeyNotFound`

**Fields:** `key: String`

**When:** A KV operation requires a key that doesn't exist (context-dependent — `kv_get` returns `Ok(None)` instead).

**Handle:** Check existence with `kv_get` first, or match this variant.

### `BranchNotFound`

**Fields:** `branch: String`

**When:** `set_branch()` is called with a branch name that doesn't exist.

**Handle:** Create the branch first with `create_branch()`, or check with `branches().exists()`.

### `CollectionNotFound`

**Fields:** `collection: String`

**When:** A vector operation targets a collection that doesn't exist.

**Handle:** Create the collection with `vector_create_collection()`.

### `StreamNotFound`

**Fields:** `stream: String`

**When:** An event operation targets a stream that doesn't exist.

### `CellNotFound`

**Fields:** `cell: String`

**When:** A state operation requires a cell that doesn't exist.

### `DocumentNotFound`

**Fields:** `key: String`

**When:** A JSON operation requires a document that doesn't exist.

## Validation Errors

### `WrongType`

**Fields:** `expected: String`, `actual: String`

**When:** A value has the wrong type for an operation.

### `InvalidKey`

**Fields:** `reason: String`

**When:** A key string is malformed or exceeds limits.

### `InvalidPath`

**Fields:** `reason: String`

**When:** A JSON path is syntactically invalid.

### `InvalidInput`

**Fields:** `reason: String`

**When:** General input validation failure.

## Concurrency Errors

### `VersionConflict`

**Fields:** `expected: u64`, `actual: u64`

**When:** A CAS operation's expected version doesn't match the actual current version.

**Handle:** Read the current version and retry.

### `TransactionConflict`

**Fields:** `reason: String`

**When:** Transaction commit fails because a concurrent transaction modified data you read.

**Handle:** Retry the entire transaction.

### `TransitionFailed`

**Fields:** `expected: String`, `actual: String`

**When:** A state transition's expected current value doesn't match.

### `Conflict`

**Fields:** `reason: String`

**When:** General concurrency conflict.

## State Errors

### `BranchExists`

**Fields:** `branch: String`

**When:** `create_branch()` is called with a name that already exists.

**Handle:** Use a different name, or skip creation if the branch already exists.

### `BranchClosed`

**Fields:** `branch: String`

**When:** An operation targets a branch that has been closed.

### `CollectionExists`

**Fields:** `collection: String`

**When:** `vector_create_collection()` is called with a name that already exists.

## Constraint Errors

### `DimensionMismatch`

**Fields:** `expected: usize`, `actual: usize`

**When:** A vector's dimension doesn't match the collection's dimension.

**Handle:** Ensure your embedding dimension matches the collection configuration.

### `ConstraintViolation`

**Fields:** `reason: String`

**When:** An operation violates a constraint, such as:
- Deleting the default branch
- Deleting the current branch

### `HistoryTrimmed`

**Fields:** `requested: u64`, `earliest: u64`

**When:** A versioned read requests a version that has been removed by retention policy.

### `Overflow`

**Fields:** `reason: String`

**When:** A numeric operation would overflow.

## Transaction Errors

### `TransactionNotActive`

**When:** `TxnCommit` or `TxnRollback` is called without an active transaction.

### `TransactionAlreadyActive`

**When:** `TxnBegin` is called while a transaction is already active.

### `TransactionConflict`

**Fields:** `reason: String`

**When:** Commit-time validation detects conflicts with concurrent transactions.

## System Errors

### `Io`

**Fields:** `reason: String`

**When:** File system operations fail (disk full, permissions, etc.).

### `Serialization`

**Fields:** `reason: String`

**When:** Data serialization or deserialization fails.

### `Internal`

**Fields:** `reason: String`

**When:** A bug or invariant violation. If you see this, please file an issue.

### `NotImplemented`

**Fields:** `feature: String`, `reason: String`

**When:** A feature is not yet available. Currently applies to:
- `fork_branch` — branch forking
- `diff_branches` — branch diffing
