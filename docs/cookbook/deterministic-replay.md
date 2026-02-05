# Deterministic Replay

This recipe shows how to record nondeterministic inputs (API responses, timestamps, random values) into the Event Log so you can replay an agent session deterministically.

## Pattern

During a live session, the agent:
1. Before making an external call, records the intent
2. After receiving the response, records the result in the Event Log
3. All decisions are based on recorded data

During replay, the agent reads from the Event Log instead of making external calls.

## Implementation

### Recording Phase

Record all external inputs as events during the live session:

```bash
#!/bin/bash
set -euo pipefail

DB="--db ./data"
SESSION="session-001"

# Create isolated branch for this session
strata $DB branch create "$SESSION"

# Record external API response
API_RESPONSE=$(curl -s https://api.weather.com/current)  # nondeterministic
strata $DB --branch "$SESSION" event append external_input "{\"source\":\"weather_api\",\"response\":$API_RESPONSE}"

# Record a random decision
RANDOM_CHOICE=$((RANDOM % 3))
strata $DB --branch "$SESSION" event append external_input "{\"source\":\"random\",\"value\":$RANDOM_CHOICE}"

# Record timestamp
NOW=$(date +%s)
strata $DB --branch "$SESSION" event append external_input "{\"source\":\"timestamp\",\"value\":$NOW}"

# Use the recorded values for agent logic
strata $DB --branch "$SESSION" kv put decision "chose option $RANDOM_CHOICE"
strata $DB --branch "$SESSION" state set status completed
```

### Replay Phase

Read all external inputs from the Event Log and replay:

```bash
#!/bin/bash
set -euo pipefail

DB="--db ./data"
SESSION="session-001"

# Read all external inputs in order
strata $DB --branch "$SESSION" event list external_input

# The agent can re-execute its logic using recorded inputs
# and produce the exact same results
```

### Interactive Replay

You can also replay interactively:

```
$ strata --db ./data
strata:default/default> use session-001
strata:session-001/default> event list external_input
seq=1 type=external_input payload={"source":"weather_api","response":{...}}
seq=2 type=external_input payload={"source":"random","value":2}
seq=3 type=external_input payload={"source":"timestamp","value":1706900000}
strata:session-001/default> kv get decision
"chose option 2"
strata:session-001/default> state get status
"completed"
```

## See Also

- [Event Log Guide](../guides/event-log.md) — event append and read operations
- [Branch Management Guide](../guides/branch-management.md) — branch-per-session pattern
- [Agent State Management](agent-state-management.md) — full session pattern
