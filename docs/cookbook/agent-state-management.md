# Agent State Management

This recipe shows how to manage a complete AI agent session using multiple primitives: KV for configuration, StateCell for status tracking, and EventLog for audit trails.

## Pattern

Each agent session gets its own branch. Within that branch:
- **KV Store** holds agent configuration and working memory
- **State Cell** tracks the agent's current status
- **Event Log** records every action for auditing and replay

## Implementation

Set up and run an agent session in the REPL:

```
$ strata --cache
strata:default/default> branch create session-001
OK
strata:default/default> use session-001
strata:session-001/default> kv put config:model gpt-4
(version) 1
strata:session-001/default> kv put config:max_steps 10
(version) 1
strata:session-001/default> kv put config:temperature 0.7
(version) 1
strata:session-001/default> state init status started
(version) 1
strata:session-001/default> state init step_count 0
(version) 1
strata:session-001/default> state set status thinking
(version) 2
strata:session-001/default> event append tool_call '{"step":0,"action":"web_search","query":"step 0 query"}'
(seq) 1
strata:session-001/default> kv put result:step:0 "Result from step 0"
(version) 1
strata:session-001/default> state set step_count 1
(version) 2
strata:session-001/default> state set status completed
(version) 3
strata:session-001/default> event len
1
strata:session-001/default> state get status
"completed"
strata:session-001/default> state get step_count
1
```

Or as a shell script for automated sessions:

```bash
#!/bin/bash
set -euo pipefail

DB="--db ./data"
SESSION="session-001"

# Create isolated branch
strata $DB branch create "$SESSION"

# Configuration
strata $DB --branch "$SESSION" kv put config:model gpt-4
strata $DB --branch "$SESSION" kv put config:max_steps 10
strata $DB --branch "$SESSION" kv put config:temperature 0.7

# Initialize status
strata $DB --branch "$SESSION" state init status started
strata $DB --branch "$SESSION" state init step_count 0

# Agent loop
for step in $(seq 0 9); do
    strata $DB --branch "$SESSION" state set status thinking
    strata $DB --branch "$SESSION" event append tool_call "{\"step\":$step,\"action\":\"web_search\"}"
    strata $DB --branch "$SESSION" kv put "result:step:$step" "Result from step $step"
    strata $DB --branch "$SESSION" state set step_count $((step + 1))
done

# Mark complete
strata $DB --branch "$SESSION" state set status completed

echo "Session complete"
strata $DB --branch "$SESSION" event len
strata $DB --branch "$SESSION" state get status
```

## Reading Session History

After a session completes, switch to its branch and read everything:

```
$ strata --db ./data
strata:default/default> use session-001
strata:session-001/default> event list tool_call
seq=1 type=tool_call payload={"step":0,"action":"web_search","query":"step 0 query"}
...
strata:session-001/default> kv get config:model
"gpt-4"
strata:session-001/default> state get status
"completed"
```

## Cleanup

Delete sessions you no longer need:

```bash
strata --db ./data branch del session-001
```

## See Also

- [KV Store Guide](../guides/kv-store.md)
- [Event Log Guide](../guides/event-log.md)
- [State Cell Guide](../guides/state-cell.md)
- [Branch Management Guide](../guides/branch-management.md)
