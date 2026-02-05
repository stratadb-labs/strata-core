# State Cell Guide

State cells provide mutable, named values with **compare-and-swap (CAS)** for safe concurrent coordination. Use them for counters, locks, state machines, and any value that multiple writers need to update safely.

## Command Overview

| Command | Syntax | Returns |
|---------|--------|---------|
| `state set` | `state set <cell> <value>` | Version number |
| `state get` | `state get <cell>` | Current value, or `(nil)` |
| `state init` | `state init <cell> <value>` | Version number (or no-op) |
| `state cas` | `state cas <cell> <expected> <value>` | New version, or error on mismatch |
| `state list` | `state list [--prefix P]` | All cells |
| `state history` | `state history <cell>` | Version history |

## Set (Unconditional Write)

`state set` overwrites the cell value regardless of its current state:

```
$ strata --cache
strata:default/default> state set status active
(version) 1
strata:default/default> state set counter 0
(version) 1
strata:default/default> state get status
"active"
```

## Read

`state get` returns the current value, or `(nil)` if the cell doesn't exist:

```
$ strata --cache
strata:default/default> state get missing
(nil)
strata:default/default> state set cell 42
(version) 1
strata:default/default> state get cell
42
```

## Init (Create If Absent)

`state init` sets the value only if the cell does not already exist. This is idempotent — calling it multiple times with different values has no effect after the first call:

```
$ strata --cache
strata:default/default> state init status idle
(version) 1
strata:default/default> state init status should-not-overwrite
(no-op)
strata:default/default> state get status
"idle"
```

## Compare-and-Swap (CAS)

`state cas` updates a cell only if the current version counter matches the expected value. This prevents lost updates when multiple writers are competing:

```
$ strata --cache
strata:default/default> state set lock free
(version) 1
strata:default/default> state cas lock 1 acquired
(version) 2
strata:default/default> state cas lock 1 stolen
(error) CAS conflict: expected version 1, current version 2
```

### CAS for Create-If-Absent

Pass `none` as the expected version to create a cell only if it doesn't exist:

```
$ strata --cache
strata:default/default> state cas new-cell none initial
(version) 1
```

## Common Patterns

### State Machine

```
$ strata --cache
strata:default/default> state set task:status pending
(version) 1
strata:default/default> state cas task:status 1 running
(version) 2
strata:default/default> state cas task:status 2 completed
(version) 3
```

### Simple Lock

```
$ strata --cache
strata:default/default> state cas lock:resource none owner-1
(version) 1
strata:default/default> state get lock:resource
"owner-1"
strata:default/default> state set lock:resource free
(version) 2
```

### Counter with CAS Retry (Shell Script)

```bash
#!/bin/bash
set -euo pipefail

DB="--db ./data"

# Initialize counter
strata $DB state set counter 0

# CAS increment loop
while true; do
    current=$(strata $DB --raw state get counter)
    version=$(strata $DB --json state get counter | jq -r '.version')
    new_value=$((current + 1))
    if strata $DB state cas counter "$version" "$new_value" 2>/dev/null; then
        echo "Counter incremented to $new_value"
        break
    fi
    echo "CAS conflict, retrying..."
done
```

## Branch Isolation

State cells are isolated by branch, like all primitives.

## Space Isolation

Within a branch, state cells are scoped to the current space:

```
$ strata --cache
strata:default/default> state set status active
(version) 1
strata:default/default> use default other
strata:default/other> state get status
(nil)
```

See [Spaces](spaces.md) for the full guide.

## Transactions

State cell operations (read, init, CAS) participate in transactions.

See [Sessions and Transactions](sessions-and-transactions.md) for details.

## Next

- [JSON Store](json-store.md) — structured documents
- [Cookbook: Multi-Agent Coordination](../cookbook/multi-agent-coordination.md) — CAS patterns for agent coordination
