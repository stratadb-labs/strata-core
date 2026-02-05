# Sessions and Transactions Guide

This guide covers the transaction commands for multi-operation atomicity. For the conceptual overview, see [Concepts: Transactions](../concepts/transactions.md).

## Transactions in the CLI

When no transaction is active, commands are executed directly. When a transaction is active, data commands route through the transaction with read-your-writes semantics.

```
$ strata --db ./data
strata:default/default>
```

## Transaction Lifecycle

### Begin

```
$ strata --cache
strata:default/default> begin
OK
```

After begin, the CLI takes a snapshot of the current database state. All reads within the transaction see this snapshot plus your own writes.

You can also begin a read-only transaction:

```
strata:default/default> begin --read-only
OK
```

### Execute Commands

Data commands (KV, Event, State, JSON) route through the transaction:

```
$ strata --cache
strata:default/default> begin
OK
strata:default/default> kv put key-a 1
(version) 1
strata:default/default> kv get key-a
1
```

### Commit

```
$ strata --cache
strata:default/default> begin
OK
strata:default/default> kv put key value
(version) 1
strata:default/default> commit
OK
```

If a conflict occurs:

```
strata:default/default> commit
(error) TransactionConflict: key was modified by another transaction
```

### Rollback

Explicitly abort a transaction:

```
strata:default/default> begin
OK
strata:default/default> kv put key value
(version) 1
strata:default/default> rollback
OK
```

All uncommitted changes are discarded.

### Auto-Rollback on Exit

If the CLI session ends (quit, Ctrl-C) while a transaction is active, the transaction is automatically rolled back.

## Transaction Scope

### Transactional Commands

These commands route through the transaction when one is active:

| Primitive | Commands |
|-----------|----------|
| **KV** | `kv get`, `kv put`, `kv del`, `kv list` |
| **Event** | `event append`, `event get`, `event len` |
| **State** | `state get`, `state init`, `state cas` |
| **JSON** | `json set`, `json get`, `json del` |

### Non-Transactional Commands

These always execute directly, regardless of transaction state:

| Category | Commands |
|----------|----------|
| **Vector** | All vector operations |
| **Branch** | `branch create`, `branch info`, `branch list`, `branch exists`, `branch del` |
| **Database** | `ping`, `info`, `flush`, `compact` |

## Query Transaction State

```
$ strata --cache
strata:default/default> begin
OK
strata:default/default> txn info
status: active
strata:default/default> txn active
true
strata:default/default> commit
OK
strata:default/default> txn active
false
```

## Multi-Primitive Atomicity

Transactions span all transactional primitives. You can atomically update KV, State, and Event in a single transaction:

```
$ strata --cache
strata:default/default> begin
OK
strata:default/default> kv put config:version 2
(version) 1
strata:default/default> state set status updated
(version) 1
strata:default/default> event append config_change '{"version":2}'
(seq) 1
strata:default/default> commit
OK
```

All three changes commit atomically.

## Conflict Retry Pattern

When a transaction conflicts, retry the entire operation. In a shell script:

```bash
#!/bin/bash
set -euo pipefail

for attempt in 1 2 3 4 5; do
    strata --db ./data <<'EOF' && break
begin
kv get counter
kv put counter 1
commit
EOF
    echo "Conflict on attempt $attempt, retrying..."
done
```

In the REPL:

```
$ strata --db ./data
strata:default/default> begin
OK
strata:default/default> kv get counter
42
strata:default/default> kv put counter 43
(version) 1
strata:default/default> commit
(error) TransactionConflict: key was modified by another transaction
strata:default/default> begin
OK
strata:default/default> kv get counter
43
strata:default/default> kv put counter 44
(version) 1
strata:default/default> commit
OK
```

## Error States

| Error | Cause |
|-------|-------|
| `TransactionAlreadyActive` | Called `begin` while a transaction is already open |
| `TransactionNotActive` | Called `commit` or `rollback` without an active transaction |
| `TransactionConflict` | Commit-time validation found conflicts with concurrent changes |

## Next

- [Search](search.md) — hybrid keyword + semantic search
- [Error Handling](error-handling.md) — error categories and patterns
