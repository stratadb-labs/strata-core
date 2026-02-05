# Error Handling Guide

All StrataDB CLI commands return an error message and a non-zero exit code on failure. Errors have structured categories so you can handle them in scripts.

## CLI Error Output

Errors are displayed with the error category:

```
$ strata --cache
strata:default/default> kv get missing
(nil)
strata:default/default> use nonexistent
(error) BranchNotFound: branch "nonexistent" does not exist
```

In shell mode, check exit codes:

```bash
if strata --cache branch exists my-branch; then
    echo "Branch exists"
else
    echo "Branch does not exist"
fi
```

## Error Categories

### Not Found Errors

Returned when an entity doesn't exist.

| Error | When |
|-------|------|
| `BranchNotFound` | Branch doesn't exist |
| `CollectionNotFound` | Vector collection doesn't exist |

Note: `kv get` returns `(nil)` for missing keys, not an error. Similarly for `state get` and `json get`.

### Validation Errors

Returned when input is malformed.

| Error | When |
|-------|------|
| `InvalidKey` | Key format is invalid |
| `InvalidPath` | JSON path is malformed |
| `InvalidInput` | General input validation failure |

### Concurrency Errors

Returned when concurrent operations conflict.

| Error | When |
|-------|------|
| `VersionConflict` | CAS version doesn't match |
| `TransactionConflict` | Commit-time validation failure |

### State Errors

Returned when an operation violates state constraints.

| Error | When |
|-------|------|
| `BranchExists` | Creating a branch that already exists |
| `CollectionExists` | Creating a collection that already exists |

### Constraint Errors

Returned when limits or constraints are violated.

| Error | When |
|-------|------|
| `DimensionMismatch` | Vector dimension doesn't match collection |
| `ConstraintViolation` | General constraint violation (e.g., deleting default branch) |

### Transaction Errors

Returned during transaction lifecycle issues.

| Error | When |
|-------|------|
| `TransactionNotActive` | Commit/rollback without an active transaction |
| `TransactionAlreadyActive` | Begin while a transaction is already open |
| `TransactionConflict` | Commit-time validation failure |

## Common Patterns

### Handle Specific Errors in Scripts

```bash
#!/bin/bash
set -euo pipefail

# Create branch only if it doesn't exist
if ! strata --cache branch create my-branch 2>/dev/null; then
    echo "Branch already exists (or other error)"
fi
```

### Transaction Retry

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

### Idempotent Operations

Use `state init` for idempotent initialization and check for `BranchExists`:

```bash
# Create branch only if it doesn't exist
strata --cache branch create session-001 2>/dev/null || true

# Initialize state idempotently
strata --cache state init status idle
```

## Next

- [Error Reference](../reference/error-reference.md) — complete error specification
- [Sessions and Transactions](sessions-and-transactions.md) — transaction patterns
