# Sessions and Transactions Guide

This guide covers the `Session` API for multi-operation atomic transactions. For the conceptual overview, see [Concepts: Transactions](../concepts/transactions.md).

## Sessions

A `Session` wraps a database and manages an optional open transaction. When no transaction is active, commands are executed directly. When a transaction is active, data commands route through the transaction with read-your-writes semantics.

```rust
use stratadb::{Strata, Command, Output, Value};

let db = Strata::open("./data")?;
let mut session = db.session();
```

## Transaction Lifecycle

### Begin

```rust
session.execute(Command::TxnBegin {
    branch: None,      // Uses default branch
    options: None,  // Default options
})?;
```

After begin, the session takes a snapshot of the current database state. All reads within the transaction see this snapshot plus your own writes.

### Execute Commands

Data commands (KV, Event, State, JSON) route through the transaction:

```rust
// Write
session.execute(Command::KvPut {
    branch: None,
    key: "key-a".into(),
    value: Value::Int(1),
})?;

// Read your own write
let output = session.execute(Command::KvGet {
    branch: None,
    key: "key-a".into(),
})?;
// Returns the value you just wrote
```

### Commit

```rust
match session.execute(Command::TxnCommit) {
    Ok(Output::TxnCommitted { version }) => {
        println!("Committed at version {}", version);
    }
    Err(Error::TransactionConflict { reason }) => {
        println!("Conflict: {}", reason);
        // Transaction was rolled back — retry if needed
    }
    Err(e) => return Err(e),
}
```

### Rollback

Explicitly abort a transaction:

```rust
session.execute(Command::TxnRollback)?;
// All uncommitted changes are discarded
```

### Auto-Rollback on Drop

If a `Session` is dropped while a transaction is active, the transaction is automatically rolled back:

```rust
{
    let mut session = Session::new(db.clone());
    session.execute(Command::TxnBegin { branch: None, options: None })?;
    session.execute(Command::KvPut {
        branch: None,
        key: "key".into(),
        value: Value::Int(1),
    })?;
    // session dropped — transaction rolled back
}
```

## Transaction Scope

### Transactional Commands

These commands route through the transaction when one is active:

| Primitive | Commands |
|-----------|----------|
| **KV** | KvGet, KvPut, KvDelete, KvList |
| **Event** | EventAppend, EventRead, EventLen |
| **State** | StateRead, StateInit, StateCas |
| **JSON** | JsonSet, JsonGet, JsonDelete |

### Non-Transactional Commands

These always execute directly, regardless of transaction state:

| Category | Commands |
|----------|----------|
| **Vector** | All vector operations |
| **Branch** | BranchCreate, BranchGet, BranchList, BranchExists, BranchDelete |
| **Database** | Ping, Info, Flush, Compact |
| **Retention** | RetentionApply, RetentionStats, RetentionPreview |

## Query Transaction State

```rust
// Check if a transaction is active
let is_active = session.in_transaction();

// Get transaction info
let output = session.execute(Command::TxnInfo)?;
if let Output::TxnInfo(Some(info)) = output {
    println!("Transaction ID: {}", info.id);
    println!("Status: {:?}", info.status);
}

// Check via command
let output = session.execute(Command::TxnIsActive)?;
```

## Multi-Primitive Atomicity

Transactions span all transactional primitives. You can atomically update KV, State, and Event in a single transaction:

```rust
session.execute(Command::TxnBegin { branch: None, options: None })?;

// KV write
session.execute(Command::KvPut {
    branch: None,
    key: "config:version".into(),
    value: Value::Int(2),
})?;

// State update
session.execute(Command::StateSet {
    branch: None,
    cell: "status".into(),
    value: Value::String("updated".into()),
})?;

// Event log
session.execute(Command::EventAppend {
    branch: None,
    event_type: "config_change".into(),
    payload: serde_json::json!({"version": 2}).into(),
})?;

// All three commit atomically
session.execute(Command::TxnCommit)?;
```

## Conflict Retry Pattern

When a transaction conflicts, retry the entire operation:

```rust
loop {
    session.execute(Command::TxnBegin { branch: None, options: None })?;

    // Read current state
    let output = session.execute(Command::KvGet {
        branch: None,
        key: "counter".into(),
    })?;

    let current = match output {
        Output::Maybe(Some(v)) => v.as_int().unwrap_or(0),
        _ => 0,
    };

    // Modify and write back
    session.execute(Command::KvPut {
        branch: None,
        key: "counter".into(),
        value: Value::Int(current + 1),
    })?;

    match session.execute(Command::TxnCommit) {
        Ok(_) => break,
        Err(Error::TransactionConflict { .. }) => continue,
        Err(e) => return Err(e),
    }
}
```

## Error States

| Error | Cause |
|-------|-------|
| `TransactionAlreadyActive` | Called `TxnBegin` while a transaction is already open |
| `TransactionNotActive` | Called `TxnCommit` or `TxnRollback` without an active transaction |
| `TransactionConflict` | Commit-time validation found conflicts with concurrent changes |

## Next

- [Search](search.md) — hybrid keyword + semantic search
- [Error Handling](error-handling.md) — error categories and patterns
