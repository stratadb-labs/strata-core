# Agent State Management

This recipe shows how to manage a complete AI agent session using multiple primitives: KV for configuration, StateCell for status tracking, and EventLog for audit trails.

## Pattern

Each agent session gets its own branch. Within that branch:
- **KV Store** holds agent configuration and working memory
- **State Cell** tracks the agent's current status
- **Event Log** records every action for auditing and replay

## Implementation

```rust
use stratadb::{Strata, Value};

fn run_agent_session(db: &mut Strata, session_id: &str) -> stratadb::Result<()> {
    // Create an isolated branch for this session
    db.create_branch(session_id)?;
    db.set_branch(session_id)?;

    // === Configuration (KV) ===
    db.kv_put("config:model", "gpt-4")?;
    db.kv_put("config:max_steps", 10i64)?;
    db.kv_put("config:temperature", 0.7)?;

    // === Status Tracking (StateCell) ===
    db.state_init("status", "started")?;
    db.state_init("step_count", 0i64)?;

    // === Main Agent Loop ===
    for step in 0..10 {
        // Update status
        db.state_set("status", "thinking")?;

        // Record the tool call (EventLog)
        let payload: Value = serde_json::json!({
            "step": step,
            "action": "web_search",
            "query": format!("step {} query", step),
        }).into();
        db.event_append("tool_call", payload)?;

        // Store intermediate results (KV)
        db.kv_put(
            &format!("result:step:{}", step),
            format!("Result from step {}", step),
        )?;

        // Update step counter
        db.state_set("step_count", (step + 1) as i64)?;
    }

    // Mark session complete
    db.state_set("status", "completed")?;

    // Review the session
    let status = db.state_read("status")?;
    let events = db.event_len()?;
    let steps = db.state_read("step_count")?;
    println!("Session {}: status={:?}, events={}, steps={:?}",
        session_id, status, events, steps);

    // Switch back to default branch
    db.set_branch("default")?;

    Ok(())
}
```

## Reading Session History

After a session completes, you can switch to its branch and read everything:

```rust
fn review_session(db: &mut Strata, session_id: &str) -> stratadb::Result<()> {
    db.set_branch(session_id)?;

    // Read all tool calls
    let tool_calls = db.event_read_by_type("tool_call")?;
    for tc in &tool_calls {
        println!("Tool call: {:?}", tc.value);
    }

    // Read config
    let model = db.kv_get("config:model")?;
    println!("Model used: {:?}", model);

    // Read final status
    let status = db.state_read("status")?;
    println!("Final status: {:?}", status);

    db.set_branch("default")?;
    Ok(())
}
```

## Cleanup

Delete sessions you no longer need:

```rust
db.set_branch("default")?;
db.delete_branch("session-001")?; // Removes all data
```

## See Also

- [KV Store Guide](../guides/kv-store.md)
- [Event Log Guide](../guides/event-log.md)
- [State Cell Guide](../guides/state-cell.md)
- [Branch Management Guide](../guides/branch-management.md)
