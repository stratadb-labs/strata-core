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

```rust
use stratadb::{Strata, Value};

fn live_session(db: &mut Strata, session_id: &str) -> stratadb::Result<()> {
    db.create_branch(session_id)?;
    db.set_branch(session_id)?;

    // Record external API response
    let api_response = call_external_api("weather"); // nondeterministic
    let payload: Value = serde_json::json!({
        "source": "weather_api",
        "response": api_response,
    }).into();
    db.event_append("external_input", payload)?;

    // Record a random decision
    let random_choice: i64 = rand::random::<i64>() % 3;
    let payload: Value = serde_json::json!({
        "source": "random",
        "value": random_choice,
    }).into();
    db.event_append("external_input", payload)?;

    // Record timestamp
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let payload: Value = serde_json::json!({
        "source": "timestamp",
        "value": now,
    }).into();
    db.event_append("external_input", payload)?;

    // Use the recorded values for agent logic
    db.kv_put("decision", format!("chose option {}", random_choice))?;
    db.state_set("status", "completed")?;

    db.set_branch("default")?;
    Ok(())
}

fn call_external_api(query: &str) -> String {
    format!("response for {}", query) // placeholder
}
```

### Replay Phase

```rust
fn replay_session(db: &mut Strata, session_id: &str) -> stratadb::Result<()> {
    db.set_branch(session_id)?;

    // Read all external inputs in order
    let inputs = db.event_read_by_type("external_input")?;

    for (i, input) in inputs.iter().enumerate() {
        println!("Input {}: {:?}", i, input.value);

        // Extract the recorded values and use them
        // instead of making live external calls
    }

    // The agent can now re-execute its logic using recorded inputs
    // and produce the exact same results

    db.set_branch("default")?;
    Ok(())
}
```

## Helper: Input Recorder

A reusable abstraction for recording and replaying inputs:

```rust
struct InputRecorder<'a> {
    db: &'a Strata,
    replay_inputs: Option<Vec<Value>>,
    replay_index: usize,
}

impl<'a> InputRecorder<'a> {
    /// Create a recorder for live operation
    fn live(db: &'a Strata) -> Self {
        Self { db, replay_inputs: None, replay_index: 0 }
    }

    /// Create a recorder for replay (reads from event log)
    fn replay(db: &'a Strata) -> stratadb::Result<Self> {
        let inputs = db.event_read_by_type("external_input")?;
        let values: Vec<Value> = inputs.into_iter().map(|v| v.value).collect();
        Ok(Self { db, replay_inputs: Some(values), replay_index: 0 })
    }

    /// Record an external input (live) or read the next one (replay)
    fn record_input(&mut self, source: &str, live_value: Value) -> stratadb::Result<Value> {
        if let Some(ref inputs) = self.replay_inputs {
            // Replay mode: return the recorded value
            let value = inputs[self.replay_index].clone();
            self.replay_index += 1;
            Ok(value)
        } else {
            // Live mode: record and return
            let payload: Value = serde_json::json!({
                "source": source,
                "value": serde_json::Value::from(live_value.clone()),
            }).into();
            self.db.event_append("external_input", payload)?;
            Ok(live_value)
        }
    }
}
```

## See Also

- [Event Log Guide](../guides/event-log.md) — event append and read operations
- [Branch Management Guide](../guides/branch-management.md) — branch-per-session pattern
- [Agent State Management](agent-state-management.md) — full session pattern
