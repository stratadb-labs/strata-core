# Multi-Agent Coordination

This recipe shows how multiple agents can coordinate through a shared StrataDB database using State Cells for CAS-based coordination and branches for isolation.

## Pattern 1: Shared Database, Same Branch (CAS Coordination)

Multiple agents share a branch and use compare-and-swap to coordinate. Each agent tries to claim the next task using CAS:

```bash
#!/bin/bash
set -euo pipefail

DB="--db ./data"
AGENT_ID="$1"

# Initialize task counter (idempotent)
strata $DB state init task:next 0

# CAS loop to claim a task
while true; do
    current=$(strata $DB --raw state get task:next)
    version=$(strata $DB --json state get task:next | jq -r '.version')
    next=$((current + 1))

    if strata $DB state cas task:next "$version" "$next" 2>/dev/null; then
        echo "Agent $AGENT_ID claimed task $current"
        strata $DB kv put "task:${current}:owner" "$AGENT_ID"
        strata $DB kv put "task:${current}:status" in_progress
        break
    fi
    echo "CAS failed, retrying..."
done
```

## Pattern 2: Different Branches (Full Isolation)

Each agent works in its own branch, then results are aggregated:

```bash
#!/bin/bash
set -euo pipefail

DB="--db ./data"

# Create a branch per agent
for i in 0 1 2; do
    strata $DB branch create "agent-$i"
    strata $DB --branch "agent-$i" kv put result "Agent $i result"
    strata $DB --branch "agent-$i" state set status done
done

# Aggregate results
for i in 0 1 2; do
    result=$(strata $DB --branch "agent-$i" --raw kv get result)
    echo "Agent $i: $result"
done
```

## Pattern 3: Leader Election

Use StateCell CAS for simple leader election:

```bash
#!/bin/bash
set -euo pipefail

DB="--db ./data"
AGENT_ID="$1"

# Try to create the leader cell (only succeeds if it doesn't exist)
if strata $DB state cas leader none "$AGENT_ID" 2>/dev/null; then
    echo "$AGENT_ID is now the leader"
else
    current=$(strata $DB --raw state get leader)
    echo "$AGENT_ID is not the leader (current: $current)"
fi
```

## See Also

- [State Cell Guide](../guides/state-cell.md) — CAS semantics
- [Branch Management Guide](../guides/branch-management.md) — creating and switching branches
- [Transactions Guide](../guides/sessions-and-transactions.md) — atomic multi-key operations
