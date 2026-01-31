# Transactions

StrataDB uses **Optimistic Concurrency Control (OCC)** with snapshot isolation. Transactions read from a consistent snapshot and validate at commit time — no locks are held during execution.

## How OCC Works

1. **Begin** — the transaction takes a snapshot of the current state
2. **Read/Write** — operations execute against the snapshot with read-your-writes semantics
3. **Validate** — at commit time, StrataDB checks for conflicts with concurrent transactions
4. **Commit or Abort** — if no conflicts, changes become visible; otherwise, the transaction aborts

This means:
- **Reads never block.** You always see a consistent snapshot.
- **Writes never block other writers.** Conflicts are detected at commit time, not at write time.
- **Agents rarely conflict.** In practice, agents working on different keys or different branches never conflict.

## Using Transactions

Transactions are managed through the `Session` API:

```rust
use stratadb::{Strata, Session, Command, Value};
use std::sync::Arc;
use stratadb::strata_engine::Database;

// Create a session from a shared database
let db = Arc::new(Database::open("./data")?);
let mut session = Session::new(db);

// Begin a transaction
session.execute(Command::TxnBegin { branch: None, options: None })?;

// All data operations now go through the transaction
session.execute(Command::KvPut {
    branch: None,
    key: "key".into(),
    value: Value::Int(42),
})?;

// Read your own writes
let output = session.execute(Command::KvGet {
    branch: None,
    key: "key".into(),
})?;

// Commit — validates and applies atomically
session.execute(Command::TxnCommit)?;
```

## Snapshot Isolation

Within a transaction, you see a **consistent snapshot** of the database as it was when the transaction began, plus your own writes:

- **Read-your-writes**: If you write a key, subsequent reads in the same transaction see your write.
- **No dirty reads**: You never see uncommitted changes from other transactions.
- **No phantom reads**: The set of keys matching a prefix doesn't change during your transaction (from your perspective).

## Conflict Detection

At commit time, StrataDB checks whether any key you read has been modified by another committed transaction since your snapshot was taken:

| Your Operation | Concurrent Operation | Conflict? |
|---------------|---------------------|-----------|
| Read key A | Write key A | Yes |
| Write key A | Write key A | Yes (first committer wins) |
| Read key A | Write key B | No |
| Write key A | Read key A (other) | No (the other transaction will conflict if it tries to commit) |
| Blind write A | Blind write A | No (blind writes don't add to read set) |

**First committer wins**: if two transactions write the same key, whichever commits first succeeds. The second gets a conflict error.

## Transaction Scope

These operations route through the transaction when one is active:

| Primitive | Operations |
|-----------|-----------|
| **KV** | Get, Put, Delete, List |
| **Event** | Append, Read, Len |
| **State** | Read, Init, CAS |
| **JSON** | Set, Get, Delete |

These operations **always bypass** the transaction:

| Category | Operations |
|----------|-----------|
| **Vector** | All vector operations |
| **Branch** | Create, Get, List, Exists, Delete |
| **Database** | Ping, Info, Flush, Compact |

## Error Handling

When a transaction conflicts, you get a `TransactionConflict` error. The standard pattern is to retry:

```rust
loop {
    session.execute(Command::TxnBegin { branch: None, options: None })?;

    // ... your operations ...

    match session.execute(Command::TxnCommit) {
        Ok(_) => break, // Success
        Err(Error::TransactionConflict { .. }) => {
            // Transaction was automatically rolled back — retry
            continue;
        }
        Err(e) => return Err(e), // Unexpected error
    }
}
```

## Auto-Rollback

If a `Session` is dropped while a transaction is active, the transaction is automatically rolled back. No uncommitted changes leak.

```rust
{
    let mut session = Session::new(db.clone());
    session.execute(Command::TxnBegin { branch: None, options: None })?;
    session.execute(Command::KvPut { branch: None, key: "key".into(), value: Value::Int(1) })?;
    // session dropped here — transaction automatically rolled back
}
// "key" was never committed
```

## When to Use Transactions

| Scenario | Use Transaction? |
|----------|-----------------|
| Single key read or write | No — individual operations are already atomic |
| Read-modify-write on a key | Yes — prevents lost updates |
| Write to multiple keys atomically | Yes — all-or-nothing semantics |
| Cross-primitive consistency | Yes — KV + StateCell + EventLog in one atomic commit |
| Vector operations | No — vectors bypass transactions |

## Next

- [Durability](durability.md) — how data is persisted
- [Sessions and Transactions Guide](../guides/sessions-and-transactions.md) — full API walkthrough
